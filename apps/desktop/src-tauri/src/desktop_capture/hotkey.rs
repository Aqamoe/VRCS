use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use tauri::{AppHandle, Manager as _};
use windows_sys::Win32::{
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT},
        WindowsAndMessaging::{
            GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY,
            WM_QUIT,
        },
    },
};

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Ocr,
    Vision(u64),
}

impl Action {
    fn error(self, code: &str) -> String {
        let prefix = match self {
            Self::Ocr => "desktop_ocr",
            Self::Vision(_) => "desktop_vision",
        };
        format!("{prefix}.{code}")
    }

    fn report(self, app: &AppHandle, error: Option<String>) {
        match self {
            Self::Ocr => app
                .state::<crate::desktop_ocr::Manager>()
                .shortcut_error(error),
            Self::Vision(lifecycle) => app
                .state::<crate::desktop_vision::Manager>()
                .shortcut_error_for(lifecycle, error),
        }
    }

    fn trigger(self, app: &AppHandle) {
        match self {
            Self::Ocr => {
                let _ = app.state::<crate::desktop_ocr::Manager>().scan();
            }
            Self::Vision(lifecycle) => {
                let _ = app
                    .state::<crate::desktop_vision::Manager>()
                    .capture_for(lifecycle);
            }
        }
    }
}

pub struct Hotkey {
    sender: Sender<(Option<String>, Option<String>)>,
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl Hotkey {
    pub fn new(app: AppHandle, action: Action) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel::<(Option<String>, Option<String>)>();
        let (ready, thread_id) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("vrcs-desktop-hotkey".into())
            .spawn(move || {
                let mut message: MSG = unsafe { std::mem::zeroed() };
                unsafe {
                    PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                }
                if ready.send(unsafe { GetCurrentThreadId() }).is_err() {
                    return;
                }
                let mut registered = false;
                while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
                    if message.message == WM_APP {
                        for (shortcut, preflight_error) in receiver.try_iter() {
                            if registered {
                                unsafe {
                                    UnregisterHotKey(std::ptr::null_mut(), 1);
                                }
                            }
                            registered = false;
                            let error =
                                match shortcut.as_deref().map(super::shortcut::parse_shortcut) {
                                    None => None,
                                    Some(Err(_)) => Some(action.error("invalid_shortcut")),
                                    Some(Ok((modifiers, key))) => {
                                        registered = unsafe {
                                            RegisterHotKey(
                                                std::ptr::null_mut(),
                                                1,
                                                modifiers | MOD_NOREPEAT,
                                                key,
                                            ) != 0
                                        };
                                        if !registered {
                                            tracing::warn!(?action, error = %std::io::Error::last_os_error(), "RegisterHotKey failed");
                                        }
                                        (!registered).then(|| action.error("shortcut_conflict"))
                                    }
                                };
                            let error = error.or(preflight_error);
                            if let Some(error) = &error {
                                tracing::warn!(?action, %error, "Desktop screenshot shortcut registration failed");
                            } else if registered {
                                tracing::info!(?action, "Desktop screenshot shortcut registered");
                            }
                            action.report(&app, error);
                        }
                    } else if message.message == WM_HOTKEY && registered {
                        let app = app.clone();
                        let dispatcher = app.clone();
                        let _ = dispatcher.run_on_main_thread(move || action.trigger(&app));
                    }
                }
                if registered {
                    unsafe {
                        UnregisterHotKey(std::ptr::null_mut(), 1);
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        let thread_id = thread_id.recv().map_err(|error| error.to_string())?;
        Ok(Self {
            sender,
            thread_id,
            worker: Some(worker),
        })
    }

    pub fn update(&self, shortcut: Option<String>) -> Result<(), String> {
        self.update_with_error(shortcut, None)
    }

    pub fn update_with_error(
        &self,
        shortcut: Option<String>,
        error: Option<String>,
    ) -> Result<(), String> {
        self.sender
            .send((shortcut, error))
            .map_err(|error| error.to_string())?;
        if unsafe { PostThreadMessageW(self.thread_id, WM_APP, 0, 0) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
