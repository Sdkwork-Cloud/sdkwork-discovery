use sdkwork_utils_rust::sha256_hash;

pub(crate) fn content_hash(value: &str) -> String {
    sha256_hash(value.as_bytes())
}
