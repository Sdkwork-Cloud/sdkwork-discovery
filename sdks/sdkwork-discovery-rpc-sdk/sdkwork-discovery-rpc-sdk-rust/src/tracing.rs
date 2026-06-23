use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

const TRACE_ID_LENGTH: usize = 32;
const SPAN_ID_LENGTH: usize = 16;

static SPAN_COUNTER: AtomicU64 = AtomicU64::new(0);

fn normalize_hex(value: &str, expected_length: usize, label: &str) -> Result<String, String> {
    let normalized = value.replace('-', "").to_lowercase();
    if normalized.len() != expected_length
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "{label} must be {expected_length} hex characters"
        ));
    }
    Ok(normalized)
}

fn random_span_id() -> String {
    let seed = SPAN_COUNTER.fetch_add(1, Ordering::Relaxed) ^ std::process::id() as u64;
    format!("{seed:016x}")
}

pub fn create_traceparent(trace_id: &str, parent_span_id: Option<&str>) -> Result<String, String> {
    let normalized_trace_id = normalize_hex(trace_id, TRACE_ID_LENGTH, "traceId")?;
    let normalized_span_id = match parent_span_id {
        Some(span_id) => normalize_hex(span_id, SPAN_ID_LENGTH, "parentSpanId")?,
        None => random_span_id(),
    };
    Ok(format!(
        "00-{normalized_trace_id}-{normalized_span_id}-01"
    ))
}

pub fn create_traceparent_metadata(
    trace_id: &str,
    parent_span_id: Option<&str>,
) -> Result<HashMap<String, String>, String> {
    Ok(HashMap::from([(
        "traceparent".to_string(),
        create_traceparent(trace_id, parent_span_id)?,
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_traceparent_formats_w3c_value() {
        let traceparent = create_traceparent(
            "0af7651916cd43dd8448eb211c80319c",
            Some("b7ad6b7169203331"),
        )
        .unwrap();

        assert_eq!(
            traceparent,
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        );
    }

    #[test]
    fn create_traceparent_rejects_invalid_trace_id() {
        assert!(create_traceparent("not-a-trace-id", None).is_err());
    }
}
