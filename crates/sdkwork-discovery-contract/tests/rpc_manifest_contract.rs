use sdkwork_discovery_contract::RpcContractInventory;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn rpc_manifest_covers_every_proto_service_method() {
    let inventory = RpcContractInventory::load_from_workspace(env!("CARGO_MANIFEST_DIR"))
        .expect("rpc contract inventory should load");

    let missing_from_manifest = inventory.proto_methods_missing_from_manifest();
    let missing_from_proto = inventory.manifest_methods_missing_from_proto();

    assert!(
        missing_from_manifest.is_empty(),
        "proto methods missing from manifest: {missing_from_manifest:?}"
    );
    assert!(
        missing_from_proto.is_empty(),
        "manifest methods missing from proto: {missing_from_proto:?}"
    );
}

#[test]
fn rpc_manifest_declares_required_sdkwork_metadata() {
    let inventory = RpcContractInventory::load_from_workspace(env!("CARGO_MANIFEST_DIR"))
        .expect("rpc contract inventory should load");

    let violations = inventory.manifest_standard_violations();

    assert!(
        violations.is_empty(),
        "rpc manifest standard violations: {violations:?}"
    );
}

#[test]
fn rpc_manifest_rejects_semantically_mismatched_method_metadata() {
    let fixture_root = manifest_semantics_fixture_root();
    write_fixture_proto(
        &fixture_root,
        r#"
syntax = "proto3";

package sdkwork.discovery.internal.v1;

service RegistryService {
  rpc WatchService(WatchServiceRequest) returns (WatchServiceResponse);
  rpc RetrieveInstance(RetrieveInstanceRequest) returns (RetrieveInstanceResponse);
}

message WatchServiceRequest {}
message WatchServiceResponse {}
message RetrieveInstanceRequest {}
message RetrieveInstanceResponse {}
"#,
    );
    write_fixture_manifest(
        &fixture_root,
        r#"
{
  "schemaVersion": 1,
  "kind": "sdkwork.rpc.manifest",
  "domain": "platform",
  "capability": "discovery",
  "sdkFamily": "sdkwork-discovery-rpc-sdk",
  "protoRoots": ["../../proto"],
  "services": [
    {
      "package": "sdkwork.discovery.internal.v1",
      "service": "RegistryService",
      "surface": "internal",
      "owner": "sdkwork-platform",
      "methods": [
        {
          "method": "WatchService",
          "operationId": "discovery.registry.services.watch",
          "auth": "backend-operator",
          "idempotency": "required",
          "streaming": "unary",
          "compatibility": "stable"
        },
        {
          "method": "RetrieveInstance",
          "operationId": "discovery.registry.instances.retrieve",
          "auth": "service-identity",
          "idempotency": "required",
          "streaming": "server",
          "compatibility": "stable"
        }
      ]
    }
  ]
}
"#,
    );

    let inventory = RpcContractInventory::load_from_roots(
        &fixture_root.join("proto/sdkwork/discovery"),
        &fixture_root.join("sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc.manifest.json"),
    )
    .expect("fixture rpc contract inventory should load");
    let violations = inventory.manifest_standard_violations();

    assert!(violations.contains(&"sdkwork.discovery.internal.v1.RegistryService surface internal methods must use auth service-identity".to_owned()));
    assert!(violations.contains(&"sdkwork.discovery.internal.v1.RegistryService.WatchService watch methods must use server streaming".to_owned()));
    assert!(violations.contains(&"sdkwork.discovery.internal.v1.RegistryService.WatchService watch methods must use idempotency none".to_owned()));
    assert!(violations.contains(&"sdkwork.discovery.internal.v1.RegistryService.RetrieveInstance read methods must use unary streaming".to_owned()));
    assert!(violations.contains(&"sdkwork.discovery.internal.v1.RegistryService.RetrieveInstance read methods must use idempotency none".to_owned()));
}

fn manifest_semantics_fixture_root() -> PathBuf {
    workspace_root()
        .join("target")
        .join("test-generated")
        .join("sdkwork-discovery")
        .join("rpc-manifest-semantic-mismatch")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("contract crate must live below workspace root")
        .to_path_buf()
}

fn write_fixture_proto(fixture_root: &Path, content: &str) {
    let proto_path = fixture_root
        .join("proto")
        .join("sdkwork")
        .join("discovery")
        .join("internal")
        .join("v1")
        .join("fixture.proto");
    let parent = proto_path.parent().expect("proto path should have parent");
    fs::create_dir_all(parent).expect("fixture proto directory should be created");
    fs::write(proto_path, content).expect("fixture proto should be written");
}

fn write_fixture_manifest(fixture_root: &Path, content: &str) {
    let manifest_path = fixture_root
        .join("sdks")
        .join("sdkwork-discovery-rpc-sdk")
        .join("sdkwork-discovery-rpc.manifest.json");
    let parent = manifest_path
        .parent()
        .expect("manifest path should have parent");
    fs::create_dir_all(parent).expect("fixture manifest directory should be created");
    fs::write(manifest_path, content).expect("fixture manifest should be written");
}
