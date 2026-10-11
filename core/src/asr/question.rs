use std::sync::Arc;

use crate::config::{AsrConfig, QWEN_MANAGED_BACKEND};
use crate::providers::{self, ServiceAdapter};

use super::{
    share_audio, spawn_cloud_recognition_session, spawn_managed_qwen_session, CloudEvent,
    CloudRecognitionSession, ModelManager, QwenRuntime, SharedAudio,
};

/// Recognize buffered 16 kHz mono PCM in an isolated session.
/// The caller must apply a timeout around this entire future, including connection setup.
pub(crate) async fn transcribe_question(
    config: AsrConfig,
    samples: Vec<f32>,
    silence_seconds: f64,
    runtime: Arc<QwenRuntime>,
    manager: Arc<ModelManager>,
) -> Result<String, String> {
    validate_audio(&samples)?;
    validate_backend(&config)?;
    let session = if config.backend == QWEN_MANAGED_BACKEND {
        spawn_managed_qwen_session(config, runtime, manager).await?
    } else {
        spawn_cloud_recognition_session(config, silence_seconds).await?
    };
    recognize_session(session, share_audio(samples)).await
}

fn validate_audio(samples: &[f32]) -> Result<(), String> {
    if samples.is_empty() {
        return Err("Question audio is empty".into());
    }
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err("Question audio contains non-finite samples".into());
    }
    let power = samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>()
        / samples.len() as f64;
    // Reject digital silence and negligible capture noise before contacting a model.
    if power <= 1e-8 {
        return Err("Question audio is silent".into());
    }
    Ok(())
}

pub(crate) fn validate_backend(config: &AsrConfig) -> Result<(), String> {
    if config.backend == QWEN_MANAGED_BACKEND {
        return Ok(());
    }
    let (_, service) = providers::recognition_service(&config.backend)
        .ok_or_else(|| format!("Unsupported question ASR backend: {}", config.backend))?;
    match service.adapter {
        ServiceAdapter::OpenAiAudioTranscriptions
        | ServiceAdapter::QwenRealtime
        | ServiceAdapter::FunAsrRealtime => Ok(()),
        ServiceAdapter::QwenLiveTranslate
        | ServiceAdapter::OpenAiRealtimeTranslate
        | ServiceAdapter::GeminiLiveTranslate => {
            Err("Question ASR does not support live translation backends; a source transcript is required".into())
        }
        _ => Err(format!(
            "Unsupported question ASR backend {}: reliable end-of-audio completion is not available",
            config.backend
        )),
    }
}

struct QuestionSession(CloudRecognitionSession);

impl Drop for QuestionSession {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub(super) async fn recognize_session(
    session: CloudRecognitionSession,
    samples: SharedAudio,
) -> Result<String, String> {
    // Keep ownership here while awaiting so timeout/drop always cancels the worker.
    let mut session = QuestionSession(session);
    let events = match &mut session.0 {
        CloudRecognitionSession::Realtime(stream) => {
            stream.transcribe_question_audio(samples).await?
        }
        CloudRecognitionSession::Segmented(upload) => {
            upload.send(samples).await?;
            let expected_id = upload.commit().await?;
            let mut events = Vec::new();
            loop {
                let event = upload.recv().await.ok_or_else(|| {
                    "Question recognition closed without a final result".to_string()
                })?;
                let complete = matches!(&event, CloudEvent::Final { utterance_id, .. } if utterance_id == &expected_id);
                if let CloudEvent::Failed { code, detail, .. } = &event {
                    return Err(format!("{code}: {detail}"));
                }
                events.push(event);
                if complete {
                    break events;
                }
            }
        }
    };
    merge_finals(events)
}

fn merge_finals(events: Vec<CloudEvent>) -> Result<String, String> {
    let mut utterances: Vec<(String, Option<String>)> = Vec::new();
    for event in events {
        let (id, text) = match event {
            CloudEvent::Partial { utterance_id, .. } => (utterance_id, None),
            CloudEvent::Final {
                utterance_id, text, ..
            } => (utterance_id, Some(text.trim().to_owned())),
            CloudEvent::Failed { code, detail, .. } => return Err(format!("{code}: {detail}")),
            CloudEvent::LiveTranslation { .. } => {
                return Err(
                    "Question recognition returned translation instead of source text".into(),
                );
            }
        };
        if let Some((_, previous)) = utterances
            .iter_mut()
            .find(|(previous_id, _)| previous_id == &id)
        {
            if text.is_some() {
                *previous = text;
            }
        } else {
            utterances.push((id, text));
        }
    }
    if utterances.iter().any(|(_, text)| text.is_none()) {
        return Err("Question recognition finished with an incomplete utterance".into());
    }
    let text = utterances
        .into_iter()
        .filter_map(|(_, text)| text.filter(|text| !text.is_empty()))
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        return Err("Question recognition returned no final source text".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn final_event(id: &str, text: &str) -> CloudEvent {
        CloudEvent::Final {
            utterance_id: id.into(),
            text: text.into(),
            language: None,
        }
    }

    #[test]
    fn finals_replace_duplicates_and_preserve_utterance_order() {
        assert_eq!(
            merge_finals(vec![
                CloudEvent::Partial {
                    utterance_id: "first".into(),
                    text: "preview".into(),
                    language: None
                },
                final_event("second", " world "),
                final_event("first", "old"),
                final_event("first", "hello"),
            ])
            .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn incomplete_empty_and_failed_results_are_errors() {
        assert!(merge_finals(vec![CloudEvent::Partial {
            utterance_id: "one".into(),
            text: "preview".into(),
            language: None,
        }])
        .is_err());
        assert!(merge_finals(vec![final_event("one", " ")]).is_err());
        assert!(merge_finals(vec![
            final_event("one", "valid"),
            CloudEvent::Failed {
                utterance_id: None,
                reset_session: true,
                code: "failure".into(),
                detail: "broken".into(),
            }
        ])
        .unwrap_err()
        .contains("broken"));
    }

    #[test]
    fn rejects_empty_silent_and_invalid_pcm() {
        assert!(validate_audio(&[]).is_err());
        assert!(validate_audio(&[0.0; 320]).is_err());
        assert!(validate_audio(&[1e-6; 320]).is_err());
        assert!(validate_audio(&[f32::NAN]).is_err());
        assert!(validate_audio(&[f32::INFINITY]).is_err());
        assert!(validate_audio(&[0.1; 320]).is_ok());
    }

    #[test]
    fn rejects_translation_and_unreliable_realtime_backends() {
        for backend in [
            providers::SERVICE_QWEN_LIVE_TRANSLATE,
            providers::SERVICE_OPENAI_REALTIME,
        ] {
            let config = AsrConfig {
                backend: backend.into(),
                ..AsrConfig::default()
            };
            assert!(validate_backend(&config).is_err());
        }
    }
}
