//! System tray UI module

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use log::{debug, error, info};

/// Events from the tray menu
#[derive(Debug, Clone)]
pub enum TrayEvent {
    Quit,
    OpenGithub,
    OpenHytale,
}

/// Status to display in tray
#[derive(Debug, Clone)]
pub struct TrayStatus {
    pub tooltip: String,
}

impl Default for TrayStatus {
    fn default() -> Self {
        Self {
            tooltip: "Waiting for Hytale...".to_string(),
        }
    }
}

// ============================================================================
// Linux implementation using ksni (StatusNotifierItem)
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use ksni::{self, Tray, TrayService};

    struct HytaleTray {
        status: Arc<Mutex<String>>,
        event_tx: Sender<TrayEvent>,
    }

    impl Tray for HytaleTray {
        fn id(&self) -> String {
            "hytale-rpc".to_string()
        }

        fn title(&self) -> String {
            "Hytale RPC".to_string()
        }

        fn icon_name(&self) -> String {
            // Use a standard icon that should be available
            "applications-games".to_string()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            let status = self.status.lock().unwrap().clone();
            ksni::ToolTip {
                title: "Hytale Discord RPC".to_string(),
                description: status,
                icon_name: String::new(),
                icon_pixmap: Vec::new(),
            }
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;

            let status = self.status.lock().unwrap().clone();

            vec![
                StandardItem {
                    label: status,
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: "GitHub".to_string(),
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::OpenGithub);
                    }),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Hytale Website".to_string(),
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::OpenHytale);
                    }),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: "Quit".to_string(),
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::Quit);
                    }),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    pub struct SystemTray {
        event_rx: Receiver<TrayEvent>,
        status: Arc<Mutex<String>>,
        handle: ksni::Handle<HytaleTray>,
    }

    impl SystemTray {
        pub fn new() -> Result<Self> {
            let (event_tx, event_rx) = mpsc::channel();
            let status = Arc::new(Mutex::new("Waiting for Hytale...".to_string()));

            let tray = HytaleTray {
                status: status.clone(),
                event_tx,
            };

            let service = TrayService::new(tray);
            let handle = service.handle();
            service.spawn();

            info!("System tray initialized");

            Ok(Self {
                event_rx,
                status,
                handle,
            })
        }

        pub fn poll_event(&self) -> Option<TrayEvent> {
            self.event_rx.try_recv().ok()
        }

        pub fn update_status(&self, new_status: TrayStatus) {
            if let Ok(mut status) = self.status.lock() {
                *status = new_status.tooltip.clone();
            }
            // Trigger tray update
            self.handle.update(|_| {});
            debug!("Tray status updated: {}", new_status.tooltip);
        }
    }
}

// ============================================================================
// macOS/Windows implementation using tray-icon
// ============================================================================

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod desktop {
    use super::*;
    use image::RgbaImage;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    pub struct SystemTray {
        _tray: TrayIcon,
        event_rx: Receiver<TrayEvent>,
        status: Arc<Mutex<TrayStatus>>,
        status_item: MenuItem,
    }

    impl SystemTray {
        pub fn new() -> Result<Self> {
            let (event_tx, event_rx) = mpsc::channel();
            let status = Arc::new(Mutex::new(TrayStatus::default()));

            let status_item = MenuItem::new("Waiting for Hytale...", false, None);
            let separator = PredefinedMenuItem::separator();
            let github_item = MenuItem::new("GitHub", true, None);
            let hytale_item = MenuItem::new("Hytale Website", true, None);
            let separator2 = PredefinedMenuItem::separator();
            let quit_item = MenuItem::new("Quit", true, None);

            let menu = Menu::new();
            menu.append(&status_item)?;
            menu.append(&separator)?;
            menu.append(&github_item)?;
            menu.append(&hytale_item)?;
            menu.append(&separator2)?;
            menu.append(&quit_item)?;

            let icon = create_tray_icon()?;

            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Hytale Discord Rich Presence")
                .with_icon(icon)
                .build()?;

            let quit_id = quit_item.id().clone();
            let github_id = github_item.id().clone();
            let hytale_id = hytale_item.id().clone();

            std::thread::spawn(move || {
                loop {
                    if let Ok(event) = MenuEvent::receiver().recv() {
                        let tray_event = if event.id == quit_id {
                            Some(TrayEvent::Quit)
                        } else if event.id == github_id {
                            Some(TrayEvent::OpenGithub)
                        } else if event.id == hytale_id {
                            Some(TrayEvent::OpenHytale)
                        } else {
                            None
                        };

                        if let Some(evt) = tray_event {
                            if event_tx.send(evt).is_err() {
                                break;
                            }
                        }
                    }
                }
            });

            info!("System tray initialized");

            Ok(Self {
                _tray: tray,
                event_rx,
                status,
                status_item,
            })
        }

        pub fn poll_event(&self) -> Option<TrayEvent> {
            self.event_rx.try_recv().ok()
        }

        pub fn update_status(&self, new_status: TrayStatus) {
            if let Ok(mut status) = self.status.lock() {
                *status = new_status.clone();
            }
            self.status_item.set_text(&new_status.tooltip);
            debug!("Tray status updated: {}", new_status.tooltip);
        }
    }

    fn create_tray_icon() -> Result<Icon> {
        let size = 32u32;
        let mut img = RgbaImage::new(size, size);

        let blue = image::Rgba([114u8, 137, 218, 255]);
        let transparent = image::Rgba([0u8, 0, 0, 0]);

        for y in 0..size {
            for x in 0..size {
                let pixel = if (x >= 4 && x <= 8) || (x >= 24 && x <= 28) {
                    if y >= 4 && y <= 28 {
                        blue
                    } else {
                        transparent
                    }
                } else if y >= 14 && y <= 18 && x >= 8 && x <= 24 {
                    blue
                } else {
                    transparent
                };
                img.put_pixel(x, y, pixel);
            }
        }

        let rgba = img.into_raw();
        Icon::from_rgba(rgba, size, size).map_err(|e| anyhow::anyhow!("Failed to create icon: {}", e))
    }
}

// ============================================================================
// Re-export the platform-specific SystemTray
// ============================================================================

#[cfg(target_os = "linux")]
pub use linux::SystemTray;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use desktop::SystemTray;

// ============================================================================
// Cross-platform utilities
// ============================================================================

/// Show a system notification
pub fn show_notification(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .icon("applications-games")
            .show()
        {
            error!("Failed to show notification: {}", e);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show()
        {
            error!("Failed to show notification: {}", e);
        }
    }

    #[cfg(target_os = "windows")]
    {
        use winrt_notification::{Duration, Toast};
        if let Err(e) = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(title)
            .text1(body)
            .duration(Duration::Short)
            .show()
        {
            error!("Failed to show notification: {}", e);
        }
    }
}

/// Open a URL in the default browser
pub fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn();
    }
}
