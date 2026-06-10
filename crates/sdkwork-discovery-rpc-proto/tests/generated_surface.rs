use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3::discovery_admin_service_server::DiscoveryAdminServiceServer;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::discovery_config_service_server::DiscoveryConfigServiceServer;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::registry_service_server::RegistryServiceServer;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::{
    RetrieveInstanceRequest, RetrieveInstanceResponse,
};

#[test]
fn generated_proto_exports_required_service_servers_and_descriptor() {
    fn assert_registry_server<T: Send + Sync + 'static>() {
        let _ = std::any::type_name::<RegistryServiceServer<T>>();
    }

    fn assert_config_server<T: Send + Sync + 'static>() {
        let _ = std::any::type_name::<DiscoveryConfigServiceServer<T>>();
    }

    fn assert_admin_server<T: Send + Sync + 'static>() {
        let _ = std::any::type_name::<DiscoveryAdminServiceServer<T>>();
    }

    let _ = assert_registry_server::<()>;
    let _ = assert_config_server::<()>;
    let _ = assert_admin_server::<()>;
    let _ = std::any::type_name::<RetrieveInstanceRequest>();
    let _ = std::any::type_name::<RetrieveInstanceResponse>();
    assert!(!sdkwork_discovery_rpc_proto::generated::FILE_DESCRIPTOR_SET.is_empty());
}
