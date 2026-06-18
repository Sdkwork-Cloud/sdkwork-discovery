pub const RPC_SDK_PROTOCOL: &str = "rpc";
pub const GENERATED_PROTO_ROOT: &str = "generated";

pub mod sdkwork {
    pub mod discovery {
        pub mod common {
            pub mod v1 {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/generated/sdkwork.discovery.common.v1.rs"
                ));
            }
        }
        pub mod backend {
            pub mod v3 {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/generated/sdkwork.discovery.backend.v3.rs"
                ));
            }
        }
        pub mod internal {
            pub mod v1 {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/generated/sdkwork.discovery.internal.v1.rs"
                ));
            }
        }
    }
}
