use reqwest::StatusCode;
use serde_json::{json, Value};

use super::LlmError;

pub(super) async fn list_models(request: reqwest::RequestBuilder) -> Result<Vec<String>, LlmError> {
    let response = request.send().await.map_err(network_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(response_status_error(response, status).await);
    }
    let value: Value = response.json().await.map_err(invalid_response)?;
    let models = extract_model_ids(&value);
    if models.is_empty() {
        return Err(LlmError {
            code: "llm.invalid_response",
            detail: "The LLM service did not return any models".into(),
            retryable: false,
        });
    }
    Ok(models)
}

pub(super) fn network_error(error: reqwest::Error) -> LlmError {
    LlmError {
        code: network_error_code(&error),
        detail: error.to_string(),
        retryable: true,
    }
}

pub(super) fn invalid_response(error: reqwest::Error) -> LlmError {
    if error.is_timeout() || error.is_body() {
        return network_error(error);
    }
    // Response::bytes() also wraps body collection failures as Decode errors.
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        if cause.is::<std::io::Error>()
            || cause
                .downcast_ref::<serde_json::Error>()
                .is_some_and(|error| error.is_io())
            || cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|error| error.is_timeout() || error.is_body())
        {
            return network_error(error);
        }
        source = cause.source();
    }
    LlmError {
        code: "llm.invalid_response",
        detail: error.to_string(),
        retryable: false,
    }
}

pub(super) fn max_serialized_body(
    body: &Value,
    max_bytes: usize,
    limit_name: &str,
) -> Result<(), LlmError> {
    let bytes = serde_json::to_vec(body).map_err(|error| LlmError {
        code: "llm.request_failed",
        detail: format!("Could not serialize {limit_name}: {error}"),
        retryable: false,
    })?;
    if bytes.len() > max_bytes {
        return Err(LlmError {
            code: "llm.request_failed",
            detail: format!(
                "{limit_name} exceeds the serialized JSON body limit: {} bytes > {max_bytes} bytes. Reduce the image or prompt size; this request cannot be retried unchanged.",
                bytes.len()
            ),
            retryable: false,
        });
    }
    Ok(())
}

pub(super) async fn response_status_error(
    response: reqwest::Response,
    status: StatusCode,
) -> LlmError {
    let value = match response.text().await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
            json!({ "error": { "message": format!("HTTP {status} returned a non-JSON response") } })
        }),
        Err(error) => return network_error(error),
    };
    status_error(status, &value)
}

pub(super) fn status_error(status: StatusCode, value: &Value) -> LlmError {
    let detail = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("LLM request failed")
        .to_owned();
    let error_type = value
        .pointer("/error/type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (code, retryable) = match status.as_u16() {
        401 | 403 => ("llm.authentication_failed", false),
        404 if detail.to_ascii_lowercase().contains("model")
            || error_type.to_ascii_lowercase().contains("model") =>
        {
            ("llm.model_not_found", false)
        }
        404 => ("llm.path_not_found", false),
        408 => ("llm.timeout", true),
        429 => ("llm.rate_limited", true),
        500..=599 => ("llm.provider_unavailable", true),
        _ => ("llm.request_failed", false),
    };
    LlmError {
        code,
        detail,
        retryable,
    }
}

fn network_error_code(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "llm.timeout";
    }
    let detail = error.to_string().to_ascii_lowercase();
    if detail.contains("certificate") || detail.contains("tls") || detail.contains("ssl") {
        "llm.tls_failed"
    } else if detail.contains("dns")
        || detail.contains("name or service not known")
        || detail.contains("failed to lookup")
    {
        "llm.dns_failed"
    } else if detail.contains("refused") {
        "llm.connection_refused"
    } else {
        "llm.network_failed"
    }
}

fn extract_model_ids(value: &Value) -> Vec<String> {
    let entries = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array);
    let mut models = entries
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("id")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty() && model.chars().count() <= 200)
                .map(str::to_owned)
        })
        .take(500)
        .collect::<Vec<_>>();
    models.sort_by_key(|model| model.to_ascii_lowercase());
    models.dedup();
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_body_limit_counts_utf8_escaping_and_json_overhead() {
        let body = json!({"text": "你好\n\"\\", "image": "data:image/png;base64,cG5n"});
        let bytes = serde_json::to_vec(&body).unwrap();
        max_serialized_body(&body, bytes.len(), "Test request").unwrap();
        let error = max_serialized_body(&body, bytes.len() - 1, "Test request").unwrap_err();
        assert_eq!(error.code, "llm.request_failed");
        assert!(!error.retryable);
        assert!(error.detail.contains("serialized JSON body limit"));
        assert!(error.detail.contains("cannot be retried unchanged"));
        assert!(error.detail.contains(&bytes.len().to_string()));
    }

    #[test]
    fn extracts_and_normalizes_model_ids() {
        let value = json!({
            "data": [
                {"id": "deepseek-reasoner"},
                {"id": " deepseek-chat "},
                {"id": "deepseek-chat"},
                {"id": ""}
            ]
        });
        assert_eq!(
            extract_model_ids(&value),
            vec!["deepseek-chat", "deepseek-reasoner"]
        );
    }

    #[test]
    fn classifies_model_related_not_found_errors() {
        let error = status_error(
            StatusCode::NOT_FOUND,
            &json!({"error": {"message": "The model does not exist"}}),
        );

        assert_eq!(error.code, "llm.model_not_found");
    }

    #[test]
    fn preserves_path_not_found_for_missing_routes() {
        let error = status_error(
            StatusCode::NOT_FOUND,
            &json!({"error": {"message": "Route not found"}}),
        );

        assert_eq!(error.code, "llm.path_not_found");
    }
}
