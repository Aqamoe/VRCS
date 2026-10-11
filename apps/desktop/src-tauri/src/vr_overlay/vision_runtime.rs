use super::{
    backend::{OpenVrBackend, OverlayKind},
    ocr_capture::encode_png,
    ocr_input_state::TriggerUpdate,
    ocr_selection::Selection,
    ocr_wrist::{Action, LaserEvent, Pointer, View, ViewMode},
    ocr_wrist_renderer::{self, Button},
    renderer::Texture,
};
use std::{sync::mpsc, time::Instant};
use vrcs_core::{
    vision::{VisionAnswer, VisionProgress, VisionService, VisionSession},
    VisionConfig, VrOcrConfig, VrOcrDisplayMode,
};

enum Update {
    Progress(u64, VisionProgress),
    Finished(u64, Result<VisionAnswer, String>),
}

pub struct VisionRuntime {
    service: Option<VisionService>,
    configuration: Option<VisionConfig>,
    session: Option<VisionSession>,
    image: Option<tauri::async_runtime::JoinHandle<Result<Vec<u8>, String>>>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    started: Option<Instant>,
    finished: Option<Instant>,
    id: u64,
    sender: mpsc::Sender<Update>,
    receiver: mpsc::Receiver<Update>,
    view: Option<View>,
    rendered: Option<View>,
    buttons: Vec<Button>,
    pages: usize,
    pointer: Pointer,
}

impl VisionRuntime {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            service: None,
            configuration: None,
            session: None,
            image: None,
            task: None,
            started: None,
            finished: None,
            id: 0,
            sender,
            receiver,
            view: None,
            rendered: None,
            buttons: Vec::new(),
            pages: 1,
            pointer: Pointer::default(),
        }
    }

    pub fn set_service(&mut self, service: VisionService) {
        self.service = Some(service);
    }

    pub fn enabled(&self) -> bool {
        self.service
            .as_ref()
            .is_some_and(|service| service.configuration().enabled)
    }

    pub fn visible(&self) -> bool {
        self.view.is_some()
    }
    pub fn busy(&self) -> bool {
        self.session.is_some() || self.task.is_some()
    }

    pub fn draft(&self) -> Option<String> {
        self.session.as_ref().map(VisionSession::draft)
    }

    pub fn clear(&mut self) {
        self.id = self.id.wrapping_add(1);
        self.session = None;
        if let Some(task) = self.image.take() {
            task.abort();
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.configuration = None;
        self.started = None;
        self.finished = None;
        self.view = None;
        self.rendered = None;
        self.buttons.clear();
        self.pages = 1;
        self.pointer = Pointer::default();
    }

    fn fail(&mut self, error: String) {
        let config = self.configuration.clone();
        let question = self
            .view
            .as_ref()
            .map(|view| view.source.clone())
            .unwrap_or_default();
        let pinned = self.view.as_ref().is_some_and(|view| view.pinned);
        self.clear();
        self.configuration = config;
        let mut view = Self::view("Error", &question, &error);
        view.pinned = pinned;
        self.view = Some(view);
        self.finished = Some(Instant::now());
    }

    fn view(status: &str, question: &str, answer: &str) -> View {
        View {
            mode: ViewMode::Vision,
            source: question.into(),
            translation: answer.into(),
            translation_label: "Answer".into(),
            title: "Visual assistant".into(),
            status: status.into(),
            block_index: 0,
            block_count: 0,
            page: 0,
            pinned: false,
            source_only: false,
            language_count: 0,
            hovered: None,
        }
    }

    pub fn tick(&mut self) {
        if self.configuration.as_ref().is_some_and(|config| {
            !self
                .service
                .as_ref()
                .is_some_and(|service| service.configuration_matches(config))
        }) {
            self.clear();
        }
        if self.session.is_some()
            && self
                .started
                .zip(self.configuration.as_ref())
                .is_some_and(|(start, config)| {
                    start.elapsed().as_secs_f64() >= f64::from(config.max_capture_seconds)
                })
        {
            self.fail("Recording timed out. Release the trigger and ask again.".into());
        }
        if let Some(error) = self.session.as_ref().and_then(VisionSession::error) {
            self.fail(error);
        }
        if let (Some(session), Some(view)) = (&self.session, &mut self.view) {
            view.source = session.draft();
        }
        while let Ok(update) = self.receiver.try_recv() {
            match update {
                Update::Progress(id, progress) if id == self.id => {
                    let (status, question) = match progress {
                        VisionProgress::FinalizingAsr => ("Finalizing speech", String::new()),
                        VisionProgress::Requesting { question } => ("Requesting answer", question),
                    };
                    if let Some(view) = &mut self.view {
                        view.status = status.into();
                        view.source = question;
                    }
                }
                Update::Finished(id, result) if id == self.id => {
                    self.task = None;
                    match result {
                        Ok(answer) => {
                            let mut view = Self::view("Ready", &answer.question, &answer.answer);
                            if let Some(previous) = &self.view {
                                view.pinned = previous.pinned;
                                view.page = previous.page;
                            }
                            self.view = Some(view);
                            self.finished = Some(Instant::now());
                        }
                        Err(error) => self.fail(error),
                    }
                }
                _ => {}
            }
        }
        if self
            .finished
            .zip(self.configuration.as_ref())
            .is_some_and(|(finished, config)| {
                !self.view.as_ref().is_some_and(|view| view.pinned)
                    && finished.elapsed().as_secs_f32() > config.display_seconds
            })
        {
            self.clear();
        }
    }

    /// Start audio synchronously before mirror readback or image encoding.
    pub fn begin(&mut self, backend: &mut OpenVrBackend, selection: &Selection) {
        self.clear();
        let Some(service) = &self.service else {
            return;
        };
        let config = service.configuration();
        self.configuration = Some(config.clone());
        let session = match service.begin() {
            Ok(session) => session,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        self.session = Some(session);
        self.started = Some(Instant::now());
        self.view = Some(Self::view("Listening · release trigger to ask", "", ""));
        // Capture always uses the left eye, independently of the OCR display mode.
        let capture_config = VrOcrConfig {
            display_mode: VrOcrDisplayMode::Wrist,
            ..VrOcrConfig::default()
        };
        let result = backend
            .capture_ocr(&capture_config, Some(selection))
            .and_then(|capture| {
                if capture.scene_pid != selection.scene_pid || capture.origin != selection.origin {
                    return Err("Scene changed before visual capture".into());
                }
                capture
                    .eyes
                    .into_iter()
                    .next()
                    .ok_or_else(|| "Missing left eye capture".into())
            });
        match result {
            Ok(eye) => {
                let selection = selection.clone();
                self.image = Some(tauri::async_runtime::spawn(async move {
                    tauri::async_runtime::spawn_blocking(move || {
                        let image = selection.crop(&eye)?.image;
                        prepare_image(image, &config)
                    })
                    .await
                    .map_err(|error| format!("Image encoding failed: {error}"))?
                }));
            }
            Err(error) => self.fail(error),
        }
    }

    pub fn trigger(&mut self, trigger: &TriggerUpdate) {
        if trigger.lost && self.session.is_some() {
            self.fail("Trigger binding or controller tracking was lost. Ask again.".into());
            return;
        }
        if !trigger.released {
            return;
        }
        let Some(mut session) = self.session.take() else {
            return;
        };
        // This must happen on the VR worker, before spawning or waiting for encoding.
        session.release();
        self.started = None;
        let Some(image) = self.image.take() else {
            self.fail("Visual image unavailable".into());
            return;
        };
        if let Some(view) = &mut self.view {
            view.status = "Finalizing speech".into();
        }
        let id = self.id;
        let sender = self.sender.clone();
        self.task = Some(tauri::async_runtime::spawn(async move {
            let result = async {
                let image = image
                    .await
                    .map_err(|error| format!("Image encoding failed: {error}"))??;
                session
                    .answer(image, |progress| {
                        let _ = sender.send(Update::Progress(id, progress));
                    })
                    .await
            }
            .await;
            let _ = sender.send(Update::Finished(id, result));
        }));
    }

    pub fn action(&mut self, action: Action) {
        if action == Action::Close {
            self.clear();
            return;
        }
        if let Some(view) = &mut self.view {
            match action {
                Action::PreviousPage => view.page = view.page.saturating_sub(1),
                Action::NextPage => view.page = (view.page + 1).min(self.pages.saturating_sub(1)),
                Action::TogglePin => view.pinned = !view.pinned,
                _ => {}
            }
            if self.finished.is_some() {
                self.finished = Some(Instant::now());
            }
        }
    }

    pub fn pointer(&mut self, events: &[LaserEvent], focused: bool) {
        for event in events.iter().copied() {
            let hovered = event.point().and_then(|[x, y]| {
                self.buttons
                    .iter()
                    .find(|button| {
                        let [left, top, right, bottom] = button.bounds;
                        x >= left && x < right && y >= top && y < bottom
                    })
                    .map(|button| button.action)
            });
            if let Some(view) = &mut self.view {
                view.hovered = hovered;
            }
            if let Some(action) = self.pointer.event(event, hovered) {
                self.action(action);
            }
        }
        if !focused || self.view.is_none() {
            self.pointer = Pointer::default();
            if let Some(view) = &mut self.view {
                view.hovered = None;
            }
        }
    }

    pub fn invalidate_render(&mut self) {
        self.rendered = None;
        self.buttons.clear();
        self.pointer = Pointer::default();
    }

    pub fn render(&mut self, backend: &mut OpenVrBackend) -> Result<bool, String> {
        let (Some(view), Some(config)) = (&mut self.view, &self.configuration) else {
            return Ok(false);
        };
        // Use an independent default, never the subtitle or OCR wrist configuration.
        let wrist = config.wrist.clone().unwrap_or_default();
        if !backend.ensure_ocr_wrist(&wrist)?.available {
            return Ok(false);
        }
        if self.rendered.as_ref() != Some(view) {
            let rendered = ocr_wrist_renderer::render(view, wrist.font_size_px, 0.9)?;
            view.page = view.page.min(rendered.page_count.saturating_sub(1));
            self.pages = rendered.page_count;
            self.buttons = rendered.buttons;
            backend.upload(OverlayKind::OcrWrist, &rendered.texture)?;
            self.rendered = Some(view.clone());
        }
        backend.set_opacity(OverlayKind::OcrWrist, wrist.opacity)?;
        backend.show(OverlayKind::OcrWrist)?;
        Ok(true)
    }
}

impl Drop for VisionRuntime {
    fn drop(&mut self) {
        self.clear();
    }
}

fn resize(image: &Texture, max_edge: u32) -> Texture {
    let scale = (max_edge.max(1) as f64 / image.width.max(image.height) as f64).min(1.0);
    let width = (image.width as f64 * scale).floor().max(1.0) as u32;
    let height = (image.height as f64 * scale).floor().max(1.0) as u32;
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        for x in 0..width {
            let source_x = u64::from(x) * u64::from(image.width) / u64::from(width);
            let source_y = u64::from(y) * u64::from(image.height) / u64::from(height);
            let source = (source_y as usize * image.width as usize + source_x as usize) * 4;
            let target = (y as usize * width as usize + x as usize) * 4;
            pixels[target..target + 4].copy_from_slice(&image.pixels[source..source + 4]);
        }
    }
    Texture {
        width,
        height,
        pixels,
    }
}

fn prepare_image(image: Texture, config: &VisionConfig) -> Result<Vec<u8>, String> {
    if image.width == 0
        || image.height == 0
        || image.width as usize * image.height as usize * 4 != image.pixels.len()
    {
        return Err("Invalid visual image".into());
    }
    let mut image = resize(&image, config.image_max_edge.min(4096));
    loop {
        let bytes = encode_png(image.clone())?;
        if bytes.len() <= config.image_max_bytes {
            return Ok(bytes);
        }
        let edge = image.width.max(image.height);
        if edge <= 1 {
            return Err("Visual image exceeds the configured byte limit".into());
        }
        image = resize(&image, (edge * 3 / 4).max(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_preserves_pixels_and_bounds() {
        let image = Texture {
            width: 4,
            height: 2,
            pixels: (0..32).collect(),
        };
        let result = resize(&image, 2);
        assert_eq!((result.width, result.height), (2, 1));
        assert_eq!(result.pixels, vec![0, 1, 2, 3, 8, 9, 10, 11]);
    }
    #[test]
    fn completed_answer_keeps_the_readers_pin_and_page() {
        let mut runtime = VisionRuntime::new();
        let mut view = VisionRuntime::view("Requesting answer", "question", "");
        view.pinned = true;
        view.page = 2;
        runtime.view = Some(view);
        runtime
            .sender
            .send(Update::Finished(
                runtime.id,
                Ok(VisionAnswer {
                    question: "question".into(),
                    answer: "answer".into(),
                }),
            ))
            .unwrap();
        runtime.tick();
        let view = runtime.view.as_ref().unwrap();
        assert!(view.pinned);
        assert_eq!(view.page, 2);
        assert_eq!(view.translation, "answer");
    }

    #[test]
    fn render_reset_keeps_the_answer_and_requires_a_fresh_upload() {
        let mut runtime = VisionRuntime::new();
        let view = VisionRuntime::view("Ready", "question", "answer");
        runtime.view = Some(view.clone());
        runtime.rendered = Some(view.clone());
        runtime.buttons.push(Button {
            action: Action::Close,
            bounds: [0.0, 0.0, 1.0, 1.0],
        });
        runtime.invalidate_render();
        assert_eq!(runtime.view.as_ref(), Some(&view));
        assert!(runtime.rendered.is_none());
        assert!(runtime.buttons.is_empty());
    }

    #[test]
    fn old_results_are_rejected_after_cancel() {
        let mut runtime = VisionRuntime::new();
        let id = runtime.id;
        runtime.clear();
        runtime
            .sender
            .send(Update::Finished(id, Err("late".into())))
            .unwrap();
        runtime.tick();
        assert!(!runtime.visible());
    }
}
