use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{DiscoveryError, DiscoveryResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcContractInventory {
    proto_methods: BTreeSet<String>,
    manifest_methods: BTreeSet<String>,
    manifest_standard_violations: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcManifestDocument {
    schema_version: u16,
    kind: String,
    domain: String,
    capability: String,
    sdk_family: String,
    proto_roots: Vec<String>,
    services: Vec<RpcManifestService>,
}

#[derive(Debug, serde::Deserialize)]
struct RpcManifestService {
    package: String,
    service: String,
    surface: String,
    owner: String,
    methods: Vec<RpcManifestMethod>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcManifestMethod {
    method: String,
    operation_id: String,
    auth: String,
    idempotency: String,
    streaming: String,
    compatibility: String,
}

impl RpcContractInventory {
    pub fn load_from_workspace(cargo_manifest_dir: &str) -> DiscoveryResult<Self> {
        let crate_dir = Path::new(cargo_manifest_dir);
        let workspace_root = crate_dir.parent().and_then(Path::parent).ok_or_else(|| {
            DiscoveryError::InvalidArgument(
                "cannot resolve workspace root from cargo manifest dir".to_string(),
            )
        })?;
        Self::load(workspace_root)
    }

    pub fn load(workspace_root: &Path) -> DiscoveryResult<Self> {
        let proto_root = workspace_root
            .join("proto")
            .join("sdkwork")
            .join("discovery");
        let manifest_path = workspace_root
            .join("sdks")
            .join("sdkwork-discovery-rpc-sdk")
            .join("sdkwork-discovery-rpc.manifest.json");

        Self::load_from_roots(&proto_root, &manifest_path)
    }

    pub fn load_from_roots(proto_root: &Path, manifest_path: &Path) -> DiscoveryResult<Self> {
        let proto_methods = collect_proto_methods(proto_root)?;
        let manifest = load_manifest(manifest_path)?;
        let manifest_methods = collect_manifest_methods(&manifest)?;
        let manifest_standard_violations = validate_manifest_standard(&manifest);

        Ok(Self {
            proto_methods,
            manifest_methods,
            manifest_standard_violations,
        })
    }

    pub fn proto_methods_missing_from_manifest(&self) -> Vec<String> {
        self.proto_methods
            .difference(&self.manifest_methods)
            .cloned()
            .collect()
    }

    pub fn manifest_methods_missing_from_proto(&self) -> Vec<String> {
        self.manifest_methods
            .difference(&self.proto_methods)
            .cloned()
            .collect()
    }

    pub fn manifest_standard_violations(&self) -> &[String] {
        &self.manifest_standard_violations
    }
}

fn collect_proto_methods(proto_root: &Path) -> DiscoveryResult<BTreeSet<String>> {
    let mut files = Vec::new();
    collect_proto_files(proto_root, &mut files)?;
    let mut methods = BTreeSet::new();

    for file in files {
        let content = fs::read_to_string(&file).map_err(|error| {
            DiscoveryError::InvalidArgument(format!(
                "failed to read proto file {}: {error}",
                file.display()
            ))
        })?;
        let package = parse_package(&content).ok_or_else(|| {
            DiscoveryError::InvalidArgument(format!("proto file lacks package: {}", file.display()))
        })?;
        let mut current_service: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(service_name) = trimmed
                .strip_prefix("service ")
                .and_then(|rest| rest.split_whitespace().next())
            {
                current_service = Some(service_name.to_string());
                continue;
            }
            if trimmed.starts_with('}') {
                current_service = None;
                continue;
            }
            if let Some(method_name) = trimmed
                .strip_prefix("rpc ")
                .and_then(|rest| rest.split('(').next())
            {
                let service = current_service.clone().ok_or_else(|| {
                    DiscoveryError::InvalidArgument(format!(
                        "rpc method outside service in {}",
                        file.display()
                    ))
                })?;
                methods.insert(format!("{package}.{service}.{method_name}"));
            }
        }
    }

    Ok(methods)
}

fn collect_proto_files(path: &Path, files: &mut Vec<PathBuf>) -> DiscoveryResult<()> {
    for entry in fs::read_dir(path).map_err(|error| {
        DiscoveryError::InvalidArgument(format!(
            "failed to read proto directory {}: {error}",
            path.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            DiscoveryError::InvalidArgument(format!(
                "failed to read proto directory entry: {error}"
            ))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_proto_files(&path, files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("proto") {
            files.push(path);
        }
    }
    Ok(())
}

fn parse_package(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("package ")
            .and_then(|rest| rest.strip_suffix(';'))
            .map(str::trim)
            .map(ToOwned::to_owned)
    })
}

fn load_manifest(manifest_path: &Path) -> DiscoveryResult<RpcManifestDocument> {
    let content = fs::read_to_string(manifest_path).map_err(|error| {
        DiscoveryError::InvalidArgument(format!(
            "failed to read rpc manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    serde_json::from_str(&content).map_err(|error| {
        DiscoveryError::InvalidArgument(format!(
            "failed to parse rpc manifest {}: {error}",
            manifest_path.display()
        ))
    })
}

fn collect_manifest_methods(manifest: &RpcManifestDocument) -> DiscoveryResult<BTreeSet<String>> {
    let mut methods = BTreeSet::new();

    for service in &manifest.services {
        for method in &service.methods {
            let key = format!("{}.{}.{}", service.package, service.service, method.method);
            if !methods.insert(key.clone()) {
                return Err(DiscoveryError::InvalidArgument(format!(
                    "duplicate rpc manifest method: {key}"
                )));
            }
        }
    }

    Ok(methods)
}

fn validate_manifest_standard(manifest: &RpcManifestDocument) -> Vec<String> {
    let mut violations = Vec::new();

    require_value(
        &mut violations,
        manifest.schema_version == 1,
        "schemaVersion must be 1",
    );
    require_value(
        &mut violations,
        manifest.kind == "sdkwork.rpc.manifest",
        "kind must be sdkwork.rpc.manifest",
    );
    require_non_empty(&mut violations, "domain", &manifest.domain);
    require_non_empty(&mut violations, "capability", &manifest.capability);
    require_value(
        &mut violations,
        manifest.sdk_family == "sdkwork-discovery-rpc-sdk",
        "sdkFamily must be sdkwork-discovery-rpc-sdk",
    );
    require_value(
        &mut violations,
        manifest
            .proto_roots
            .iter()
            .any(|root| root == "../../proto"),
        "protoRoots must include ../../proto",
    );

    for service in &manifest.services {
        let service_name = format!("{}.{}", service.package, service.service);
        require_non_empty(
            &mut violations,
            &format!("{service_name}.package"),
            &service.package,
        );
        require_non_empty(
            &mut violations,
            &format!("{service_name}.service"),
            &service.service,
        );
        require_value(
            &mut violations,
            matches!(service.surface.as_str(), "internal" | "backend"),
            &format!("{service_name}.surface must be internal or backend"),
        );
        require_non_empty(
            &mut violations,
            &format!("{service_name}.owner"),
            &service.owner,
        );
        require_value(
            &mut violations,
            !service.methods.is_empty(),
            &format!("{service_name}.methods must not be empty"),
        );

        validate_service_auth_semantics(&mut violations, service, &service_name);

        for method in &service.methods {
            let method_name = format!("{service_name}.{}", method.method);
            require_non_empty(
                &mut violations,
                &format!("{method_name}.method"),
                &method.method,
            );
            require_non_empty(
                &mut violations,
                &format!("{method_name}.operationId"),
                &method.operation_id,
            );
            require_non_empty(
                &mut violations,
                &format!("{method_name}.auth"),
                &method.auth,
            );
            require_value(
                &mut violations,
                matches!(method.idempotency.as_str(), "none" | "natural" | "required"),
                &format!("{method_name}.idempotency must be none, natural, or required"),
            );
            require_value(
                &mut violations,
                matches!(
                    method.streaming.as_str(),
                    "unary" | "server" | "client" | "bidi"
                ),
                &format!("{method_name}.streaming must be unary, server, client, or bidi"),
            );
            require_non_empty(
                &mut violations,
                &format!("{method_name}.compatibility"),
                &method.compatibility,
            );
            validate_method_metadata_semantics(&mut violations, service, method, &method_name);
        }
    }

    violations
}

fn validate_service_auth_semantics(
    violations: &mut Vec<String>,
    service: &RpcManifestService,
    service_name: &str,
) {
    let Some(expected_auth) = expected_auth_for_surface(&service.surface) else {
        return;
    };

    require_value(
        violations,
        service
            .methods
            .iter()
            .all(|method| method.auth == expected_auth),
        &format!(
            "{service_name} surface {} methods must use auth {expected_auth}",
            service.surface
        ),
    );
}

fn expected_auth_for_surface(surface: &str) -> Option<&'static str> {
    match surface {
        "internal" => Some("service-identity"),
        "backend" => Some("backend-operator"),
        _ => None,
    }
}

fn validate_method_metadata_semantics(
    violations: &mut Vec<String>,
    service: &RpcManifestService,
    method: &RpcManifestMethod,
    method_name: &str,
) {
    if is_watch_method(method) {
        require_value(
            violations,
            method.streaming == "server",
            &format!("{method_name} watch methods must use server streaming"),
        );
        require_value(
            violations,
            method.idempotency == "none",
            &format!("{method_name} watch methods must use idempotency none"),
        );
        return;
    }

    if is_read_method(method) {
        require_value(
            violations,
            method.streaming == "unary",
            &format!("{method_name} read methods must use unary streaming"),
        );
        require_value(
            violations,
            method.idempotency == "none",
            &format!("{method_name} read methods must use idempotency none"),
        );
        return;
    }

    validate_write_method_metadata(violations, service, method, method_name);
}

fn validate_write_method_metadata(
    violations: &mut Vec<String>,
    service: &RpcManifestService,
    method: &RpcManifestMethod,
    method_name: &str,
) {
    if !is_write_method(method) {
        return;
    }

    require_value(
        violations,
        method.streaming == "unary",
        &format!("{method_name} write methods must use unary streaming"),
    );

    let expected_idempotency = if service.surface == "backend" {
        "required"
    } else {
        "natural"
    };
    require_value(
        violations,
        method.idempotency == expected_idempotency,
        &format!("{method_name} write methods must use idempotency {expected_idempotency}"),
    );
}

fn is_watch_method(method: &RpcManifestMethod) -> bool {
    method.method.starts_with("Watch") || method.operation_id.ends_with(".watch")
}

fn is_read_method(method: &RpcManifestMethod) -> bool {
    method.method.starts_with("Retrieve")
        || method.method.starts_with("Discover")
        || method.method.starts_with("List")
        || method.operation_id.ends_with(".retrieve")
        || method.operation_id.ends_with(".discover")
        || method.operation_id.ends_with(".list")
}

fn is_write_method(method: &RpcManifestMethod) -> bool {
    method.method.starts_with("Create")
        || method.method.starts_with("Publish")
        || method.method.starts_with("Rollback")
        || method.method.starts_with("Register")
        || method.method.starts_with("Renew")
        || method.method.starts_with("Deregister")
        || method.method.starts_with("Report")
        || method.operation_id.ends_with(".create")
        || method.operation_id.ends_with(".publish")
        || method.operation_id.ends_with(".rollback")
        || method.operation_id.ends_with(".register")
        || method.operation_id.ends_with(".renew")
        || method.operation_id.ends_with(".deregister")
        || method.operation_id.ends_with(".report")
}

fn require_non_empty(violations: &mut Vec<String>, label: &str, value: &str) {
    require_value(
        violations,
        !value.trim().is_empty(),
        &format!("{label} must not be empty"),
    );
}

fn require_value(violations: &mut Vec<String>, condition: bool, message: &str) {
    if !condition {
        violations.push(message.to_string());
    }
}
