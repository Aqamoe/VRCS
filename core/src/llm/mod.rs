//! Provider-neutral LLM client facade.

use std::time::Instant;

use crate::config::ApiProfile;
use crate::providers::{
    self, OpenAiProtocolBehavior, ServiceAdapter, CAPABILITY_IMAGE_UNDERSTANDING,
    CAPABILITY_TEXT_GENERATION,
};

mod alibaba;
mod gemini;
mod http;
mod openai;
mod openai_compatible;
mod reasoning;

#[derive(Debug, Clone)]
pub struct LlmRequest<'a> {
    pub model: &'a str,
    pub instructions: &'a str,
    pub input: &'a str,
    pub max_output_tokens: u32,
    pub thinking_enabled: bool,
}

pub type LlmProgress = dyn Fn(&str) + Send + Sync;

#[derive(Debug, Clone, PartialEq)]
pub struct LlmError {
    pub code: &'static str,
    pub detail: String,
    pub retryable: bool,
}

#[derive(Clone)]
pub struct LlmClient {
    http: reqwest::Client,
}

impl LlmClient {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub async fn generate(
        &self,
        profile: &ApiProfile,
        api_key: &str,
        request: LlmRequest<'_>,
        on_progress: Option<&LlmProgress>,
    ) -> Result<String, LlmError> {
        self.generate_for_capability(
            profile,
            api_key,
            CAPABILITY_TEXT_GENERATION,
            request,
            on_progress,
        )
        .await
    }

    /// Generates a non-streaming text answer from text and PNG bytes.
    /// The caller is responsible for image validation and payload size limits.
    pub async fn generate_with_image(
        &self,
        profile: &ApiProfile,
        api_key: &str,
        request: LlmRequest<'_>,
        image: &[u8],
    ) -> Result<String, LlmError> {
        let resolved =
            providers::resolve_profile_capability(profile, CAPABILITY_IMAGE_UNDERSTANDING)
                .map_err(|detail| LlmError {
                    code: "llm.unsupported_provider",
                    detail,
                    retryable: false,
                })?;
        match resolved.service.adapter {
            ServiceAdapter::OpenAiChatCompletions {
                behavior: OpenAiProtocolBehavior::Standard,
            } => {
                openai_compatible::generate_with_image(
                    &self.http,
                    profile,
                    api_key,
                    request,
                    image,
                    resolved.provider.display_name,
                )
                .await
            }
            adapter => Err(unsupported_adapter(adapter)),
        }
    }

    pub(crate) async fn generate_for_capability(
        &self,
        profile: &ApiProfile,
        api_key: &str,
        capability: &str,
        request: LlmRequest<'_>,
        on_progress: Option<&LlmProgress>,
    ) -> Result<String, LlmError> {
        let started = Instant::now();
        let model = request.model.to_owned();
        let input_chars = request.input.chars().count();
        let thinking_enabled = request.thinking_enabled;
        let result = match providers::resolve_profile_capability(profile, capability) {
            Ok(resolved) => match resolved.service.adapter {
                ServiceAdapter::OpenAiResponses => {
                    openai::generate(&self.http, api_key, request).await
                }
                ServiceAdapter::OpenAiChatCompletions { behavior } => {
                    openai_compatible::generate(
                        &self.http,
                        profile,
                        api_key,
                        request,
                        resolved.provider.display_name,
                        behavior,
                        on_progress,
                    )
                    .await
                }
                ServiceAdapter::AlibabaChatCompletions => {
                    alibaba::generate(&self.http, profile, api_key, request, on_progress).await
                }
                ServiceAdapter::GeminiGenerateContent => {
                    gemini::generate(&self.http, profile, api_key, request, on_progress).await
                }
                adapter => Err(unsupported_adapter(adapter)),
            },
            Err(detail) => Err(LlmError {
                code: "llm.unsupported_provider",
                detail,
                retryable: false,
            }),
        };
        tracing::info!(
            provider = profile.provider,
            model,
            latency_ms = started.elapsed().as_millis() as u64,
            input_chars,
            output_chars = result.as_ref().map_or(0, |text| text.chars().count()),
            thinking_enabled,
            streamed = on_progress.is_some(),
            success = result.is_ok(),
            "LLM request completed"
        );
        result
    }

    pub async fn list_models(
        &self,
        profile: &ApiProfile,
        api_key: &str,
    ) -> Result<Vec<String>, LlmError> {
        let resolved = [
            CAPABILITY_TEXT_GENERATION,
            providers::CAPABILITY_TEXT_TRANSLATION,
            CAPABILITY_IMAGE_UNDERSTANDING,
        ]
        .into_iter()
        .find_map(|capability| providers::resolve_profile_capability(profile, capability).ok())
        .ok_or_else(|| LlmError {
            code: "llm.models_unsupported",
            detail: format!(
                "API profile {} has not enabled an LLM capability",
                profile.id
            ),
            retryable: false,
        })?;
        if !resolved.service.supports_model_listing {
            return Err(LlmError {
                code: "llm.models_unsupported",
                detail: format!("Service {} does not expose LLM models", resolved.service.id),
                retryable: false,
            });
        }
        self.list_models_with_adapter(profile, api_key, resolved.service.adapter)
            .await
    }

    pub async fn list_provider_models(
        &self,
        profile: &ApiProfile,
        api_key: &str,
    ) -> Result<Vec<String>, LlmError> {
        let provider = providers::definition(&profile.provider).ok_or_else(|| LlmError {
            code: "llm.models_unsupported",
            detail: format!("Unsupported API provider: {}", profile.provider),
            retryable: false,
        })?;
        let adapter = provider
            .services
            .iter()
            .find(|service| {
                service.supports_model_listing
                    && matches!(
                        service.adapter,
                        ServiceAdapter::AlibabaChatCompletions
                            | ServiceAdapter::OpenAiResponses
                            | ServiceAdapter::OpenAiChatCompletions { .. }
                            | ServiceAdapter::GeminiGenerateContent
                    )
            })
            .map(|service| service.adapter)
            .ok_or_else(|| LlmError {
                code: "llm.models_unsupported",
                detail: format!(
                    "Provider {} does not expose a model catalog",
                    profile.provider
                ),
                retryable: false,
            })?;
        self.list_models_with_adapter(profile, api_key, adapter)
            .await
    }

    async fn list_models_with_adapter(
        &self,
        profile: &ApiProfile,
        api_key: &str,
        adapter: ServiceAdapter,
    ) -> Result<Vec<String>, LlmError> {
        match adapter {
            ServiceAdapter::OpenAiResponses => {
                openai::list_models(&self.http, profile, api_key).await
            }
            ServiceAdapter::OpenAiChatCompletions { .. } => {
                openai_compatible::list_models(&self.http, profile, api_key).await
            }
            ServiceAdapter::AlibabaChatCompletions => {
                alibaba::list_models(&self.http, profile, api_key).await
            }
            ServiceAdapter::GeminiGenerateContent => gemini::list_models(&self.http, api_key).await,
            adapter => Err(unsupported_adapter(adapter)),
        }
    }

    pub async fn test_openai_compatible_streaming(
        &self,
        profile: &ApiProfile,
        api_key: &str,
        request: LlmRequest<'_>,
        on_progress: &LlmProgress,
    ) -> Result<String, LlmError> {
        let resolved = providers::resolve_profile_capability(profile, CAPABILITY_TEXT_GENERATION)
            .map_err(|detail| LlmError {
            code: "llm.models_unsupported",
            detail,
            retryable: false,
        })?;
        let ServiceAdapter::OpenAiChatCompletions { behavior } = resolved.service.adapter else {
            return Err(LlmError {
                code: "llm.models_unsupported",
                detail: "Strict streaming diagnostics require an OpenAI-compatible adapter".into(),
                retryable: false,
            });
        };
        openai_compatible::test_streaming(
            &self.http,
            profile,
            api_key,
            request,
            resolved.provider.display_name,
            behavior,
            on_progress,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApiAuthMode, HttpHeaderConfig};
    use axum::{http::HeaderMap, routing::post, Json, Router};
    use serde_json::{json, Value};

    fn request() -> LlmRequest<'static> {
        LlmRequest {
            model: "vision-model",
            instructions: "Answer briefly",
            input: "What is visible?",
            max_output_tokens: 256,
            thinking_enabled: false,
        }
    }

    #[tokio::test]
    async fn image_and_text_requests_preserve_profile_connection_and_text_behavior() {
        for auth_mode in [ApiAuthMode::Bearer, ApiAuthMode::None] {
            for with_image in [true, false] {
                let (sender, receiver) = tokio::sync::oneshot::channel();
                let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
                let app = Router::new().route(
                    "/custom/v1/chat/completions",
                    post(
                        move |headers: HeaderMap, Json(body): Json<Value>| async move {
                            sender
                                .lock()
                                .unwrap()
                                .take()
                                .unwrap()
                                .send((headers, body))
                                .unwrap();
                            Json(json!({"choices": [{"message": {"content": "Visible answer"}}]}))
                        },
                    ),
                );
                let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                    .await
                    .unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
                let profile = ApiProfile {
                    provider: providers::OPENAI_COMPATIBLE_PROVIDER.into(),
                    enabled_capabilities: vec![if with_image {
                        CAPABILITY_IMAGE_UNDERSTANDING.into()
                    } else {
                        CAPABILITY_TEXT_GENERATION.into()
                    }],
                    base_url: Some(format!("http://{address}/custom/v1/chat/completions")),
                    auth_mode,
                    timeout_ms: 1000,
                    headers: vec![HttpHeaderConfig {
                        name: "X-Test-Client".into(),
                        value: "VRCS".into(),
                    }],
                    ..ApiProfile::default()
                };
                let client = LlmClient::new(reqwest::Client::new());
                let output = if with_image {
                    client
                        .generate_with_image(&profile, "test-key", request(), b"\x89PNG\r\n\x1a\n")
                        .await
                } else {
                    client.generate(&profile, "test-key", request(), None).await
                }
                .unwrap();
                let (headers, body) = receiver.await.unwrap();
                server.abort();
                assert_eq!(output, "Visible answer");
                assert_eq!(headers["x-test-client"], "VRCS");
                if auth_mode == ApiAuthMode::Bearer {
                    assert_eq!(headers["authorization"], "Bearer test-key");
                } else {
                    assert!(!headers.contains_key("authorization"));
                }
                assert_eq!(body["model"], "vision-model");
                assert_eq!(body["max_tokens"], 256);
                assert_eq!(body["stream"], false);
                assert_eq!(
                    body["messages"][0],
                    json!({"role": "system", "content": "Answer briefly"})
                );
                assert_eq!(body["messages"][1]["role"], "user");
                if with_image {
                    assert_eq!(
                        body["messages"][1]["content"],
                        json!([
                            {"type": "text", "text": "What is visible?"},
                            {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgo="}}
                        ])
                    );
                } else {
                    assert_eq!(body["messages"][1]["content"], "What is visible?");
                }
            }
        }
    }

    #[tokio::test]
    async fn rejects_disabled_unsupported_and_empty_image_inputs_before_http() {
        let client = LlmClient::new(reqwest::Client::new());
        let mut profile = ApiProfile {
            provider: providers::OPENAI_COMPATIBLE_PROVIDER.into(),
            enabled_capabilities: vec![CAPABILITY_TEXT_GENERATION.into()],
            ..ApiProfile::default()
        };
        let error = client
            .generate_with_image(&profile, "", request(), b"png")
            .await
            .unwrap_err();
        assert_eq!(error.code, "llm.unsupported_provider");
        assert!(!error.retryable);
        profile.enabled_capabilities = vec![CAPABILITY_IMAGE_UNDERSTANDING.into()];
        for provider in providers::catalog() {
            if provider.capabilities.supports_image_understanding {
                continue;
            }
            profile.provider = provider.id.into();
            let error = client
                .generate_with_image(&profile, "", request(), b"png")
                .await
                .unwrap_err();
            assert_eq!(error.code, "llm.unsupported_provider");
            assert!(!error.retryable);
        }
        profile.provider = providers::OPENAI_COMPATIBLE_PROVIDER.into();
        let error = client
            .generate_with_image(&profile, "", request(), &[])
            .await
            .unwrap_err();
        assert_eq!(error.code, "llm.request_failed");
        assert!(!error.retryable);
    }

    #[tokio::test]
    async fn image_only_profile_can_list_models() {
        let app = Router::new().route(
            "/v1/models",
            axum::routing::get(|| async { Json(json!({"data": [{"id": "vision-model"}]})) }),
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let profile = ApiProfile {
            provider: providers::OPENAI_COMPATIBLE_PROVIDER.into(),
            enabled_capabilities: vec![CAPABILITY_IMAGE_UNDERSTANDING.into()],
            base_url: Some(format!("http://{address}/v1")),
            auth_mode: ApiAuthMode::None,
            ..ApiProfile::default()
        };
        let models = LlmClient::new(reqwest::Client::new())
            .list_models(&profile, "")
            .await
            .unwrap();
        server.abort();
        assert_eq!(models, ["vision-model"]);
    }
}

fn unsupported_adapter(adapter: ServiceAdapter) -> LlmError {
    LlmError {
        code: "llm.unsupported_provider",
        detail: format!("Unsupported LLM service adapter: {adapter:?}"),
        retryable: false,
    }
}
