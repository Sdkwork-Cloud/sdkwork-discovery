use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct RpcIdempotencyOptions {
    pub idempotency_key: Option<String>,
    pub request_hash: Option<String>,
}

pub fn create_rpc_idempotency_metadata(
    options: RpcIdempotencyOptions,
) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    if let Some(idempotency_key) = options.idempotency_key {
        metadata.insert("idempotency-key".to_string(), idempotency_key);
    }

    if let Some(request_hash) = options.request_hash {
        metadata.insert("x-request-hash".to_string(), request_hash);
    }

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_rpc_idempotency_metadata_includes_present_fields() {
        let metadata = create_rpc_idempotency_metadata(RpcIdempotencyOptions {
            idempotency_key: Some("draft-1".to_string()),
            request_hash: Some("sha256:abc".to_string()),
        });

        assert_eq!(metadata.get("idempotency-key").map(String::as_str), Some("draft-1"));
        assert_eq!(
            metadata.get("x-request-hash").map(String::as_str),
            Some("sha256:abc")
        );
    }
}
