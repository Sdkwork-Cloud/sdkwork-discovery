pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("sdkwork_discovery_descriptor");

pub mod sdkwork {
    pub mod discovery {
        pub mod common {
            pub mod v1 {
                tonic::include_proto!("sdkwork.discovery.common.v1");
            }
        }

        pub mod internal {
            pub mod v1 {
                tonic::include_proto!("sdkwork.discovery.internal.v1");
            }
        }

        pub mod backend {
            pub mod v3 {
                tonic::include_proto!("sdkwork.discovery.backend.v3");
            }
        }
    }
}
