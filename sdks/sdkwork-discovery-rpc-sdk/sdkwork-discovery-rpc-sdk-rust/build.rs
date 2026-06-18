use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let workspace_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .ok_or("sdkwork-discovery-rpc-sdk-rust must live under sdks/sdkwork-discovery-rpc-sdk/")?;
    let proto_root = workspace_root.join("proto");
    let out_dir = manifest_dir.join("generated");
    std::fs::create_dir_all(&out_dir)?;

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
        .build_server(false)
        .out_dir(&out_dir)
        .compile_protos(&protos, &[proto_root])?;

    Ok(())
}
