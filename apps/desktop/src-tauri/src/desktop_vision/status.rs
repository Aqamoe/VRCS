use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct DesktopVisionStatus {
    pub available: bool,
    pub state: &'static str,
    pub session_id: u64,
    pub revision: u64,
    pub image_id: Option<u64>,
    pub question: String,
    pub answer: String,
    pub error: Option<String>,
    pub error_detail: Option<String>,
    pub shortcut_error: Option<String>,
    pub shortcut: String,
    pub profile_name: String,
    pub model: String,
}

impl Default for DesktopVisionStatus {
    fn default() -> Self {
        Self {
            available: false,
            state: "disabled",
            session_id: 0,
            revision: 0,
            image_id: None,
            question: String::new(),
            answer: String::new(),
            error: None,
            error_detail: None,
            shortcut_error: None,
            shortcut: "Ctrl+Alt+V".into(),
            profile_name: String::new(),
            model: String::new(),
        }
    }
}

impl DesktopVisionStatus {
    pub fn busy(&self) -> bool {
        matches!(
            self.state,
            "capturing" | "selecting" | "recording" | "finalizing_asr" | "requesting"
        )
    }

    pub fn clear_error(&mut self) {
        self.error = None;
        self.error_detail = None;
    }

    pub fn fail(&mut self, code: &str, detail: Option<String>) {
        self.state = "error";
        self.error = Some(format!("desktop_vision.{code}"));
        self.error_detail = detail;
    }
}
