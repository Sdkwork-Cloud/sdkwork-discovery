use sdkwork_discovery_contract::{MemoryNamespaceStore, NamespaceConfig, NamespaceStore};

#[tokio::test]
async fn memory_namespace_store_supports_crud_and_list() {
    let mut store = MemoryNamespaceStore::new();

    store
        .create_namespace(NamespaceConfig {
            namespace: "100001".to_string(),
            max_instances: Some(100),
            max_services: Some(10),
            max_config_releases: Some(50),
            allowed_writers: vec!["writer-a".to_string()],
            allowed_readers: vec!["reader-a".to_string()],
        })
        .await
        .expect("create namespace");

    let loaded = store
        .get_namespace("100001")
        .await
        .expect("get namespace")
        .expect("namespace exists");
    assert_eq!(loaded.namespace, "100001");
    assert_eq!(loaded.max_instances, Some(100));

    store
        .update_namespace(NamespaceConfig {
            namespace: "100001".to_string(),
            max_instances: Some(200),
            max_services: Some(10),
            max_config_releases: Some(50),
            allowed_writers: vec!["writer-a".to_string()],
            allowed_readers: vec!["reader-a".to_string()],
        })
        .await
        .expect("update namespace");

    let namespaces = store.list_namespaces().await.expect("list namespaces");
    assert_eq!(namespaces.len(), 1);
    assert_eq!(namespaces[0].max_instances, Some(200));

    assert!(store.delete_namespace("100001").await.expect("delete"));
    assert!(store
        .get_namespace("100001")
        .await
        .expect("get namespace")
        .is_none());
}
