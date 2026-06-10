use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use sdkwork_discovery_storage_postgres::PostgresDiscoveryStore;

fn assert_storage_traits<T: ConfigStore + RegistryStore + WatchEventStore>() {}

#[test]
fn postgres_store_implements_all_discovery_storage_traits() {
    assert_storage_traits::<PostgresDiscoveryStore>();
}
