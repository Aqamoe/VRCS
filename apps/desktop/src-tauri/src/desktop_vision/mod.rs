mod runtime;
mod status;

pub use runtime::Manager;
use status::DesktopVisionStatus;
use tauri::{State, WebviewWindow};

fn check_window(window: &WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|_| "desktop_vision.unavailable")?;
    if matches!(window.label(), "main" | "vision") && local_url(&url) {
        Ok(())
    } else {
        Err("Unsupported visual question window".into())
    }
}

fn local_url(url: &tauri::Url) -> bool {
    (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost"))
        || (cfg!(debug_assertions)
            && url.scheme() == "http"
            && url.host_str() == Some("localhost")
            && url.port() == Some(1420))
}

#[tauri::command]
pub fn desktop_vision_status(
    window: WebviewWindow,
    manager: State<'_, Manager>,
) -> Result<DesktopVisionStatus, String> {
    check_window(&window)?;
    Ok(manager.status())
}

#[tauri::command]
pub fn desktop_vision_open(
    window: WebviewWindow,
    manager: State<'_, Manager>,
) -> Result<(), String> {
    check_window(&window)?;
    manager.open()
}

#[tauri::command]
pub fn desktop_vision_capture(
    window: WebviewWindow,
    manager: State<'_, Manager>,
) -> Result<(), String> {
    check_window(&window)?;
    manager.capture()
}

#[tauri::command]
pub fn desktop_vision_image(
    window: WebviewWindow,
    manager: State<'_, Manager>,
    image_id: u64,
) -> Result<tauri::ipc::Response, String> {
    check_window(&window)?;
    Ok(tauri::ipc::Response::new(manager.image(image_id)?))
}

#[tauri::command]
pub async fn desktop_vision_start_recording(
    window: WebviewWindow,
    manager: State<'_, Manager>,
) -> Result<(), String> {
    check_window(&window)?;
    manager.start_recording()
}

#[tauri::command]
pub fn desktop_vision_finish_recording(
    window: WebviewWindow,
    manager: State<'_, Manager>,
) -> Result<(), String> {
    check_window(&window)?;
    manager.finish_recording()
}

#[tauri::command]
pub fn desktop_vision_ask(
    window: WebviewWindow,
    manager: State<'_, Manager>,
    question: String,
    image_id: u64,
) -> Result<(), String> {
    check_window(&window)?;
    manager.ask(question, image_id)
}

#[tauri::command]
pub fn desktop_vision_cancel(
    window: WebviewWindow,
    manager: State<'_, Manager>,
) -> Result<(), String> {
    check_window(&window)?;
    manager.cancel();
    Ok(())
}

#[tauri::command]
pub fn desktop_vision_close(
    window: WebviewWindow,
    manager: State<'_, Manager>,
) -> Result<(), String> {
    check_window(&window)?;
    manager.close();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_reject_remote_origins() {
        for value in [
            "https://example.org",
            "http://localhost:1421",
            "https://tauri.localhost.example.org",
            "file:///C:/image.html",
        ] {
            assert!(!local_url(&tauri::Url::parse(value).unwrap()), "{value}");
        }
        assert!(local_url(
            &tauri::Url::parse("http://tauri.localhost/index.html?window=vision").unwrap()
        ));
        assert!(local_url(
            &tauri::Url::parse("tauri://localhost/index.html").unwrap()
        ));
    }
}
