use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let workspace_root = manifest_dir
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("sdkwork-discovery-rpc-proto must live under crates/")?;
    let proto_root = workspace_root.join("proto");
    let descriptor_path =
        PathBuf::from(std::env::var("OUT_DIR")?).join("sdkwork_discovery_descriptor.bin");

    let protos = [
        proto_root.join("sdkwork/discovery/common/v1/discovery_types.proto"),
        proto_root.join("sdkwork/discovery/internal/v1/registry_service.proto"),
        proto_root.join("sdkwork/discovery/internal/v1/discovery_config_service.proto"),
        proto_root.join("sdkwork/discovery/backend/v3/discovery_admin_service.proto"),
    ];

    for proto in &protos {
        if !proto.exists() {
            return Err(format!(
                "required discovery proto file is missing: {}",
                proto.display()
            )
            .into());
        }
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .file_descriptor_set_path(descriptor_path)
        .compile_protos(&protos, &[proto_root])?;

    Ok(())
}
