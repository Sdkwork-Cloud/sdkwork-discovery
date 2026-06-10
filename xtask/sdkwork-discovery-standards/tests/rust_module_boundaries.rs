use std::fs;
use std::path::{Path, PathBuf};

use toml::Value;

#[test]
fn rust_lib_files_are_module_assembly_boundaries() {
    let workspace_root = workspace_root();
    let mut violations = Vec::new();

    for lib_rs_path in source_lib_files(&workspace_root) {
        let source = fs::read_to_string(&lib_rs_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", lib_rs_path.display()));

        assert_module_boundary_only(&workspace_root, &lib_rs_path, &source, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "src/lib.rs files must contain only module declarations, re-exports, crate attributes, and crate docs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn runtime_storage_docs_describe_postgres_adapter_without_stale_fail_fast_claims() {
    let workspace_root = workspace_root();
    let readme = fs::read_to_string(workspace_root.join("README.md")).unwrap();
    let example_config =
        fs::read_to_string(workspace_root.join("etc/discovery.example.toml")).unwrap();

    assert!(readme.contains("postgres"));
    assert!(readme.contains("durable PostgreSQL adapter"));
    assert!(!readme.contains("postgres`: durable primary-store configuration shape, fail-fast"));
    assert!(example_config.contains("PostgreSQL durable adapter"));
    assert!(!example_config.contains("durable adapters fail fast"));
}

#[test]
fn grpc_transport_crates_and_product_binding_are_present() {
    let workspace_root = workspace_root();
    let cargo_manifest = fs::read_to_string(workspace_root.join("Cargo.toml")).unwrap();
    let product_main =
        fs::read_to_string(workspace_root.join("services/sdkwork-discovery-product/src/main.rs"))
            .unwrap();

    assert!(cargo_manifest.contains("\"crates/sdkwork-discovery-rpc-proto\""));
    assert!(cargo_manifest.contains("\"crates/sdkwork-discovery-rpc\""));
    assert!(workspace_root
        .join("crates/sdkwork-discovery-rpc-proto/build.rs")
        .exists());
    assert!(workspace_root
        .join("crates/sdkwork-discovery-rpc/src/server.rs")
        .exists());
    assert!(!product_main.contains("gRPC transport is not started in this build slice"));
}

#[test]
fn rpc_adapter_crate_has_no_forbidden_transport_or_storage_dependencies() {
    let workspace_root = workspace_root();
    let rpc_manifest =
        fs::read_to_string(workspace_root.join("crates/sdkwork-discovery-rpc/Cargo.toml")).unwrap();

    let violations = forbidden_rpc_dependency_violations(&rpc_manifest);

    assert!(
        violations.is_empty(),
        "RPC adapter crates must not depend on HTTP/Tauri adapters or direct SQLx storage:\n{}",
        violations.join("\n")
    );
}

#[test]
fn rpc_dependency_boundary_detects_direct_forbidden_dependencies() {
    let manifest = r#"
[package]
name = "sdkwork-example-rpc"
version = "0.1.0"
edition = "2021"

[dependencies]
sqlx = { workspace = true }
axum = "0.8"
sdkwork-discovery-storage-postgres = { workspace = true }

[dev-dependencies]
desktop-host = { package = "tauri", version = "2" }

[target.'cfg(unix)'.dependencies]
raw-http = { package = "reqwest", version = "0.12" }
"#;

    let violations = forbidden_rpc_dependency_violations(manifest);

    assert!(violations.contains(&"dependencies.sqlx".to_owned()));
    assert!(violations.contains(&"dependencies.axum".to_owned()));
    assert!(violations.contains(&"dependencies.sdkwork-discovery-storage-postgres".to_owned()));
    assert!(violations.contains(&"dev-dependencies.desktop-host".to_owned()));
    assert!(violations.contains(&"target.cfg(unix).dependencies.raw-http".to_owned()));
}

#[test]
fn rpc_sdk_readme_documents_standard_dual_token_metadata() {
    let workspace_root = workspace_root();
    let readme =
        fs::read_to_string(workspace_root.join("sdks/sdkwork-discovery-rpc-sdk/README.md"))
            .unwrap();

    assert!(readme.contains("authorization"));
    assert!(readme.contains("access-token"));
    assert!(readme.contains("Bearer"));
    assert!(readme.contains("x-sdkwork-subject-id"));
    assert!(readme.contains("exact instance retrieval"));
    assert!(readme.contains(
        "`DiscoveryWatchService.WatchService` requires `x-sdkwork-registry-permissions: read`"
    ));
    assert!(readme.contains(
        "`DiscoveryConfigService.WatchConfig` requires `x-sdkwork-config-permissions: read`"
    ));
    assert!(readme.contains(
        "`DiscoveryWatchService.WatchService` registry mutation events include a `ServiceInstance` payload"
    ));
    assert!(readme.contains("identity tombstone"));
    assert!(readme.contains("INSTANCE_STATUS_NOT_SERVING"));
    assert!(readme.contains("current registry state"));
    assert!(readme.contains("idempotency-key"));
    assert!(readme.contains("x-request-hash"));
    assert!(readme.contains("idempotency: \"required\""));
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("xtask crate must live below workspace root")
        .to_path_buf()
}

fn source_lib_files(workspace_root: &Path) -> Vec<PathBuf> {
    let mut lib_rs_files = Vec::new();

    collect_lib_rs(workspace_root, &mut lib_rs_files);
    lib_rs_files.sort();
    lib_rs_files
}

fn collect_lib_rs(directory: &Path, lib_rs_files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to list {}: {error}", directory.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!(
                "failed to read entry below {}: {error}",
                directory.display()
            )
        });
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        if file_name == "target" || file_name == ".git" {
            continue;
        }

        if path.is_dir() {
            collect_lib_rs(&path, lib_rs_files);
            continue;
        }

        if path.file_name().is_some_and(|name| name == "lib.rs")
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "src")
        {
            lib_rs_files.push(path);
        }
    }
}

fn assert_module_boundary_only(
    workspace_root: &Path,
    lib_rs_path: &Path,
    source: &str,
    violations: &mut Vec<String>,
) {
    for (line_number, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();

        if is_allowed_lib_line(line) {
            continue;
        }

        let relative_path = lib_rs_path
            .strip_prefix(workspace_root)
            .unwrap_or(lib_rs_path)
            .display();
        violations.push(format!("{relative_path}:{}: {line}", line_number + 1));
    }
}

fn is_allowed_lib_line(line: &str) -> bool {
    line.is_empty()
        || line.starts_with("//!")
        || line.starts_with("#![")
        || line.starts_with("pub mod ")
        || line.starts_with("mod ")
        || line.starts_with("pub use ")
        || line == "}"
        || line == "};"
        || (line.starts_with("//") && !line.starts_with("///"))
}

fn forbidden_rpc_dependency_violations(manifest_toml: &str) -> Vec<String> {
    let manifest: Value = manifest_toml
        .parse()
        .unwrap_or_else(|error| panic!("failed to parse Cargo manifest: {error}"));
    let mut violations = Vec::new();

    collect_dependency_violations(&manifest, "", &mut violations);
    violations.sort();
    violations
}

fn collect_dependency_violations(value: &Value, path: &str, violations: &mut Vec<String>) {
    let Some(table) = value.as_table() else {
        return;
    };

    for (key, child) in table {
        let child_path = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };

        if is_dependency_table(key) {
            collect_forbidden_dependencies(child, &child_path, violations);
            continue;
        }

        collect_dependency_violations(child, &child_path, violations);
    }
}

fn collect_forbidden_dependencies(
    dependency_table: &Value,
    section_path: &str,
    violations: &mut Vec<String>,
) {
    let Some(dependencies) = dependency_table.as_table() else {
        return;
    };

    for (dependency_name, dependency_value) in dependencies {
        if is_forbidden_rpc_dependency(dependency_name)
            || dependency_package_name(dependency_value).is_some_and(is_forbidden_rpc_dependency)
        {
            violations.push(format!("{section_path}.{dependency_name}"));
        }
    }
}

fn is_dependency_table(key: &str) -> bool {
    matches!(
        key,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    )
}

fn dependency_package_name(dependency_value: &Value) -> Option<&str> {
    dependency_value
        .as_table()
        .and_then(|table| table.get("package"))
        .and_then(Value::as_str)
}

fn is_forbidden_rpc_dependency(dependency_name: &str) -> bool {
    const FORBIDDEN_EXACT_DEPENDENCIES: &[&str] = &[
        "actix-web",
        "axum",
        "hyper",
        "poem",
        "reqwest",
        "rocket",
        "sqlx",
        "surf",
        "tauri",
        "tower-http",
        "ureq",
        "warp",
    ];

    FORBIDDEN_EXACT_DEPENDENCIES.contains(&dependency_name)
        || dependency_name.starts_with("sdkwork-routes-")
        || dependency_name.ends_with("-http")
        || dependency_name.ends_with("-http-rust")
        || dependency_name.ends_with("-tauri")
        || dependency_name.ends_with("-tauri-rust")
        || dependency_name.ends_with("-storage-postgres")
        || dependency_name.ends_with("-storage-sqlite")
        || dependency_name.ends_with("-storage-sqlx")
        || dependency_name.contains("-storage-sqlx-")
}
