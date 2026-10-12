#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
mod selection;
#[cfg(windows)]
pub mod selection_window;
pub mod shortcut;

#[cfg(windows)]
static PICKER_RESERVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
pub struct PickerPermit {
    _private: (),
}

#[cfg(windows)]
pub fn reserve_picker() -> Result<PickerPermit, &'static str> {
    use std::sync::atomic::Ordering;
    PICKER_RESERVED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| PickerPermit { _private: () })
        .map_err(|_| "selection_busy")
}

#[cfg(windows)]
impl Drop for PickerPermit {
    fn drop(&mut self) {
        PICKER_RESERVED.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(all(windows, test))]
mod tests {
    #[test]
    fn another_picker_or_focusing_window_cannot_take_the_reservation() {
        let permit = super::reserve_picker().unwrap();
        assert!(super::reserve_picker().is_err());
        drop(permit);
        assert!(super::reserve_picker().is_ok());
    }
}

#[cfg(windows)]
pub fn selection_hint(app: &tauri::AppHandle, window: &str) -> String {
    use tauri_plugin_store::StoreExt;
    let locale = app
        .store("preferences.json")
        .ok()
        .and_then(|store| store.get("uiLanguage"))
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|value| value != "system")
        .unwrap_or_else(|| {
            match unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() } {
                0x0404 | 0x0c04 | 0x1404 => "zh-Hant",
                0x0804 | 0x1004 => "zh-CN",
                0x0411 => "ja-JP",
                _ => "en-US",
            }
            .into()
        });
    let messages = match locale.as_str() {
        "zh-CN" => include_str!("../../../src/i18n/locales/zh-CN.json"),
        "zh-Hant" => include_str!("../../../src/i18n/locales/zh-Hant.json"),
        "ja-JP" => include_str!("../../../src/i18n/locales/ja-JP.json"),
        _ => include_str!("../../../src/i18n/locales/en-US.json"),
    };
    serde_json::from_str::<serde_json::Value>(messages)
        .ok()
        .and_then(|value| {
            value
                .pointer(&format!("/translation/{window}/selectionHint"))
                .and_then(|text| text.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Drag to select an area. Esc / right-click to cancel.".into())
}
