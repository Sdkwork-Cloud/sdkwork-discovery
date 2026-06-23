use crate::sdkwork::discovery::common::v1::{PageRequest, PageResponse};

pub const DEFAULT_DISCOVERY_PAGE_SIZE: u32 = 100;
pub const MAX_DISCOVERY_PAGE_SIZE: u32 = 200;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveryPageParams {
    pub page_size: Option<u32>,
    pub page_token: Option<String>,
}

pub fn clamp_discovery_page_size(page_size: u32) -> u32 {
    if page_size == 0 {
        DEFAULT_DISCOVERY_PAGE_SIZE
    } else {
        page_size.min(MAX_DISCOVERY_PAGE_SIZE)
    }
}

pub fn create_discovery_page_request(params: DiscoveryPageParams) -> PageRequest {
    PageRequest {
        page_size: params.page_size.unwrap_or(0),
        page_token: params.page_token.unwrap_or_default(),
    }
}

pub fn next_discovery_page_token(page: Option<&PageResponse>) -> Option<String> {
    let token = page?.next_page_token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_discovery_page_size_uses_defaults_and_max() {
        assert_eq!(clamp_discovery_page_size(0), DEFAULT_DISCOVERY_PAGE_SIZE);
        assert_eq!(clamp_discovery_page_size(50), 50);
        assert_eq!(clamp_discovery_page_size(500), MAX_DISCOVERY_PAGE_SIZE);
    }

    #[test]
    fn next_discovery_page_token_trims_blank_values() {
        assert_eq!(
            next_discovery_page_token(Some(&PageResponse {
                next_page_token: "  drive-b  ".to_string(),
            })),
            Some("drive-b".to_string())
        );
        assert_eq!(
            next_discovery_page_token(Some(&PageResponse {
                next_page_token: "   ".to_string(),
            })),
            None
        );
    }
}
