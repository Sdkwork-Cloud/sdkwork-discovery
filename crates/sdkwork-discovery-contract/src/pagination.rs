pub const DEFAULT_PAGE_SIZE: u32 = 100;
pub const MAX_PAGE_SIZE: u32 = 200;

pub fn normalize_page_size(page_size: u32) -> u32 {
    if page_size == 0 {
        DEFAULT_PAGE_SIZE
    } else {
        page_size.min(MAX_PAGE_SIZE)
    }
}

pub fn normalize_page_token(page_token: Option<String>) -> Option<String> {
    page_token.and_then(|token| {
        let trimmed = token.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

/// Paginate a pre-sorted collection using stable item keys as opaque page tokens.
pub fn paginate_sorted_keys<T, F>(
    items: Vec<T>,
    page_size: u32,
    page_token: Option<&str>,
    key_for: F,
) -> (Vec<T>, Option<String>)
where
    T: Clone,
    F: Fn(&T) -> String,
{
    let limit = normalize_page_size(page_size) as usize;
    if limit == 0 || items.is_empty() {
        return (Vec::new(), None);
    }

    let start = match page_token {
        None => 0,
        Some("") => 0,
        Some(token) => items
            .iter()
            .position(|item| key_for(item) == token)
            .map(|index| index + 1)
            .unwrap_or(items.len()),
    };

    let page_slice = items.get(start..).unwrap_or(&[]);
    if page_slice.len() <= limit {
        return (page_slice.to_vec(), None);
    }

    let page: Vec<T> = page_slice[..limit].to_vec();
    let next_page_token = page.last().map(key_for);
    (page, next_page_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_returns_next_token_when_more_items_exist() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (page, next) = paginate_sorted_keys(items, 2, None, |item| item.clone());
        assert_eq!(page, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(next.as_deref(), Some("b"));
    }

    #[test]
    fn paginate_continues_from_page_token() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (page, next) = paginate_sorted_keys(items, 2, Some("b"), |item| item.clone());
        assert_eq!(page, vec!["c".to_string()]);
        assert_eq!(next, None);
    }
}
