//! System tray UI module

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use log::{debug, error, info};

use crate::config::AppConfig;

/// Events from the tray menu
#[derive(Debug, Clone)]
pub enum TrayEvent {
    Quit,
    OpenGithub,
    OpenHytale,
    ToggleShowWorldName,
    ToggleShowServerIp,
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
        config: Arc<Mutex<AppConfig>>,
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
            let config = self.config.lock().unwrap();

            vec![
                StandardItem {
                    label: status,
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                CheckmarkItem {
                    label: "Show World Name".to_string(),
                    checked: config.show_world_name,
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::ToggleShowWorldName);
                    }),
                    ..Default::default()
                }
                .into(),
                CheckmarkItem {
                    label: "Show Server IP".to_string(),
                    checked: config.show_server_ip,
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::ToggleShowServerIp);
                    }),
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
        pub fn new(config: Arc<Mutex<AppConfig>>) -> Result<Self> {
            let (event_tx, event_rx) = mpsc::channel();
            let status = Arc::new(Mutex::new("Waiting for Hytale...".to_string()));

            let tray = HytaleTray {
                status: status.clone(),
                config,
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
        
        /// Trigger a menu rebuild to reflect config changes
        pub fn refresh_menu(&self) {
            self.handle.update(|_| {});
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
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    pub struct SystemTray {
        _tray: TrayIcon,
        event_rx: Receiver<TrayEvent>,
        status: Arc<Mutex<TrayStatus>>,
        status_item: MenuItem,
        world_name_item: CheckMenuItem,
        server_ip_item: CheckMenuItem,
    }

    impl SystemTray {
        pub fn new(config: Arc<Mutex<AppConfig>>) -> Result<Self> {
            let (event_tx, event_rx) = mpsc::channel();
            let status = Arc::new(Mutex::new(TrayStatus::default()));

            // Get initial config values
            let (show_world_name, show_server_ip) = {
                let cfg = config.lock().unwrap();
                (cfg.show_world_name, cfg.show_server_ip)
            };

            let status_item = MenuItem::new("Waiting for Hytale...", false, None);
            let separator = PredefinedMenuItem::separator();
            let world_name_item = CheckMenuItem::new("Show World Name", true, show_world_name, None);
            let server_ip_item = CheckMenuItem::new("Show Server IP", true, show_server_ip, None);
            let separator2 = PredefinedMenuItem::separator();
            let github_item = MenuItem::new("GitHub", true, None);
            let hytale_item = MenuItem::new("Hytale Website", true, None);
            let separator3 = PredefinedMenuItem::separator();
            let quit_item = MenuItem::new("Quit", true, None);

            let menu = Menu::new();
            menu.append(&status_item)?;
            menu.append(&separator)?;
            menu.append(&world_name_item)?;
            menu.append(&server_ip_item)?;
            menu.append(&separator2)?;
            menu.append(&github_item)?;
            menu.append(&hytale_item)?;
            menu.append(&separator3)?;
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
            let world_name_id = world_name_item.id().clone();
            let server_ip_id = server_ip_item.id().clone();

            std::thread::spawn(move || {
                loop {
                    if let Ok(event) = MenuEvent::receiver().recv() {
                        let tray_event = if event.id == quit_id {
                            Some(TrayEvent::Quit)
                        } else if event.id == github_id {
                            Some(TrayEvent::OpenGithub)
                        } else if event.id == hytale_id {
                            Some(TrayEvent::OpenHytale)
                        } else if event.id == world_name_id {
                            Some(TrayEvent::ToggleShowWorldName)
                        } else if event.id == server_ip_id {
                            Some(TrayEvent::ToggleShowServerIp)
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
                world_name_item,
                server_ip_item,
            })
        }

        pub fn poll_event(&self) -> Option<TrayEvent> {
            self.event_rx.try_recv().ok()
        }

        pub fn update_status(&self, new_status: TrayStatus) {
            if let Ok(mut status) = self.status.lock() {
                *status = new_status.clone();
            }
            let _ = self.status_item.set_text(&new_status.tooltip);
            debug!("Tray status updated: {}", new_status.tooltip);
        }

        pub fn refresh_menu(&self) {
            // No-op for now as CheckMenuItem toggles itself visually, 
            // and we sync the config in main loop. 
            // If we needed to force sync:
            // self.world_name_item.set_checked(config.show_world_name);
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
