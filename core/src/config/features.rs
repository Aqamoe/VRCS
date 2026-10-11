use serde::{Deserialize, Serialize};

use super::{AppConfig, GlossarySource};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeatureConfig {
    pub glossary: bool,
    pub learning: bool,
    pub anki: bool,
    pub osc_chatbox: bool,
    pub vrcx: bool,
    pub ocr: bool,
    pub visual_question_answering: bool,
    pub vr_overlay: bool,
    pub external_api: bool,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            glossary: true,
            learning: true,
            anki: true,
            osc_chatbox: true,
            vrcx: true,
            ocr: true,
            visual_question_answering: false,
            vr_overlay: true,
            external_api: true,
        }
    }
}

#[derive(Clone, Copy)]
pub enum FeatureKey {
    Glossary,
    Learning,
    Anki,
    OscChatbox,
    Vrcx,
    Ocr,
    VisualQuestionAnswering,
    VrOverlay,
    ExternalApi,
}

impl FeatureKey {
    pub fn name(self) -> &'static str {
        match self {
            Self::Glossary => "glossary",
            Self::Learning => "learning",
            Self::Anki => "anki",
            Self::OscChatbox => "osc_chatbox",
            Self::Vrcx => "vrcx",
            Self::Ocr => "ocr",
            Self::VisualQuestionAnswering => "visual_question_answering",
            Self::VrOverlay => "vr_overlay",
            Self::ExternalApi => "external_api",
        }
    }

    pub fn enabled(self, features: &FeatureConfig) -> bool {
        match self {
            Self::Glossary => features.glossary,
            Self::Learning => features.learning,
            Self::Anki => features.anki,
            Self::OscChatbox => features.osc_chatbox,
            Self::Vrcx => features.vrcx,
            Self::Ocr => features.ocr,
            Self::VisualQuestionAnswering => features.visual_question_answering,
            Self::VrOverlay => features.vr_overlay,
            Self::ExternalApi => features.external_api,
        }
    }
}

/// Apply module availability without changing the user's stored preferences.
pub fn apply_feature_gates(config: &AppConfig) -> AppConfig {
    let mut effective = config.clone();
    let features = &config.features;
    if !features.glossary {
        effective.glossary.asr_enabled = false;
        effective.glossary.llm_enabled = false;
        for source in &mut effective.glossary.sources {
            match source {
                GlossarySource::Local { enabled, .. }
                | GlossarySource::Subscription { enabled, .. } => *enabled = false,
            }
        }
    }
    effective.dictionary.selection_lookup_enabled &= features.learning;
    effective.anki.enabled &= features.anki;
    effective.osc.enabled &= features.osc_chatbox;
    effective.osc.mute_sync_enabled &= features.osc_chatbox;
    effective.osc.mute_status_toast_enabled &= features.osc_chatbox;
    effective.vrcx.enabled &= features.vrcx;
    effective.external_api.enabled &= features.external_api;
    effective.vr_overlay.enabled &= features.vr_overlay;
    effective.ocr.enabled &= features.ocr && features.vr_overlay;
    effective.ocr.desktop_enabled &= features.ocr;
    effective.vision.enabled &= features.visual_question_answering && features.vr_overlay;
    effective
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_gate_is_independent_of_ocr_and_preserves_preferences() {
        let mut config = AppConfig::default();
        config.vision.enabled = true;
        assert!(!apply_feature_gates(&config).vision.enabled);
        config.features.visual_question_answering = true;
        config.features.ocr = false;
        config.vr_overlay.enabled = false;
        assert!(apply_feature_gates(&config).vision.enabled);
        config.features.vr_overlay = false;
        assert!(!apply_feature_gates(&config).vision.enabled);
        assert!(config.vision.enabled);
        assert_eq!(
            FeatureKey::VisualQuestionAnswering.name(),
            "visual_question_answering"
        );
        assert!(FeatureKey::VisualQuestionAnswering.enabled(&config.features));
    }
}
