//! Hytale Discord Rich Presence - Rust Implementation
//!
//! A system tray application that displays your Hytale game activity on Discord.

mod config;
mod log_watcher;
mod process;
mod rpc;
mod tray;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use log::{error, info, warn};

use crate::config::{AppConfig, POLL_INTERVAL_MS};
use crate::log_watcher::LogWatcher;
use crate::process::ProcessDetector;
use crate::rpc::DiscordRpc;
use crate::tray::{open_url, show_notification, SystemTray, TrayEvent, TrayStatus};

/// Application state
struct App {
    process_detector: ProcessDetector,
    log_watcher: LogWatcher,
    discord_rpc: DiscordRpc,
    tray: Option<SystemTray>,
    config: Arc<Mutex<AppConfig>>,
    hytale_was_running: bool,
    launcher_was_running: bool,
}

impl App {
    fn new() -> Result<Self> {
        let config = Arc::new(Mutex::new(AppConfig::load()));
        Ok(Self {
            process_detector: ProcessDetector::new(),
            log_watcher: LogWatcher::new(),
            discord_rpc: DiscordRpc::new(),
            tray: None,
            config,
            hytale_was_running: false,
            launcher_was_running: false,
        })
    }

    fn init_tray(&mut self) -> Result<()> {
        match SystemTray::new(self.config.clone()) {
            Ok(tray) => {
                self.tray = Some(tray);
                info!("System tray initialized successfully");
            }
            Err(e) => {
                warn!("Failed to initialize system tray: {}. Running in CLI mode.", e);
            }
        }
        Ok(())
    }

    fn update_tray_status(&self, tooltip: &str) {
        if let Some(ref tray) = self.tray {
            tray.update_status(TrayStatus {
                tooltip: tooltip.to_string(),
            });
        } else {
            // CLI mode - print to console
            // println!("[Status] {}", tooltip); // Reduce noise
        }
    }

    fn handle_tray_events(&mut self) -> bool {
        if let Some(ref tray) = self.tray {
            while let Some(event) = tray.poll_event() {
                match event {
                    TrayEvent::Quit => {
                        info!("Quit requested from tray");
                        return true;
                    }
                    TrayEvent::OpenGithub => {
                        open_url("https://github.com/MopigamesYT/hytale-rpc-rs");
                    }
                    TrayEvent::OpenHytale => {
                        open_url("https://hytale.com");
                    }
                    TrayEvent::ToggleShowWorldName => {
                        let mut cfg = self.config.lock().unwrap();
                        cfg.show_world_name = !cfg.show_world_name;
                        if let Err(e) = cfg.save() {
                            error!("Failed to save config: {}", e);
                        }
                        info!("Toggled show_world_name to {}", cfg.show_world_name);
                        
                        // Refresh menu to show new state
                        #[cfg(target_os = "linux")]
                        tray.refresh_menu();
                    }
                    TrayEvent::ToggleShowServerIp => {
                        let mut cfg = self.config.lock().unwrap();
                        cfg.show_server_ip = !cfg.show_server_ip;
                        if let Err(e) = cfg.save() {
                            error!("Failed to save config: {}", e);
                        }
                        info!("Toggled show_server_ip to {}", cfg.show_server_ip);

                        // Refresh menu to show new state
                        #[cfg(target_os = "linux")]
                        tray.refresh_menu();
                    }
                }
            }
        }
        false
    }

    fn run(&mut self) -> Result<()> {
        info!("Starting Hytale Discord Rich Presence");
        self.update_tray_status("Waiting for Hytale...");

        loop {
            // Handle tray events
            if self.handle_tray_events() {
                break;
            }

            // Refresh process list
            self.process_detector.refresh();

            let game_running = self.process_detector.is_game_running();
            let launcher_running = self.process_detector.is_launcher_running();

            // Handle Hytale Game state changes
            if game_running && !self.hytale_was_running {
                info!("Hytale Game detected");
                self.update_tray_status("Hytale Game detected");
                show_notification("Hytale RPC", "Hytale Game detected");
            } else if !game_running && self.hytale_was_running {
                info!("Hytale Game closed");
                self.update_tray_status("Waiting for Hytale...");
                self.log_watcher.reset();
                if self.discord_rpc.is_connected() {
                    let _ = self.discord_rpc.clear();
                }
                show_notification("Hytale RPC", "Hytale Game closed");
            }
            self.hytale_was_running = game_running;

            // Handle Launcher state changes
            if launcher_running && !self.launcher_was_running {
                info!("Hytale Launcher detected");
                if !game_running {
                    self.update_tray_status("In Launcher");
                }
            } else if !launcher_running && self.launcher_was_running {
                info!("Hytale Launcher closed");
            }
            self.launcher_was_running = launcher_running;

            // Priority: Game > Launcher > None
            if game_running {
                // Core Game Logic (Log Watcher)
                if !self.discord_rpc.is_connected() {
                    if let Err(e) = self.discord_rpc.connect() {
                        warn!("Could not connect to Discord RPC: {}", e);
                        self.update_tray_status("Waiting for Discord...");
                    }
                }

                // Update log watcher
                let log_changed = self.log_watcher.update().unwrap_or_else(|e| {
                    warn!("Error reading log file: {}", e);
                    false
                });

                let state = self.log_watcher.state();
                
                // Update tray status if log changed or just periodically to reflect config?
                // We update it if log changed for now.
                if log_changed {
                    let config_guard = self.config.lock().unwrap();
                    let status = format!("{} - {}", state.details(), state.state(&config_guard));
                    self.update_tray_status(&status);
                }

                if self.discord_rpc.is_connected() {
                    let config_guard = self.config.lock().unwrap();
                    if let Err(e) = self.discord_rpc.update(state, &config_guard) {
                        error!("Failed to update Discord RPC: {}", e);
                    }
                }
            } else if launcher_running {
                // Launcher Logic
                if !self.discord_rpc.is_connected() {
                    if let Err(e) = self.discord_rpc.connect() {
                        warn!("Could not connect to Discord RPC: {}", e);
                        self.update_tray_status("Waiting for Discord...");
                    }
                }

                if self.discord_rpc.is_connected() {
                    use crate::config::GameState;
                    let state = GameState::Launcher;
                    self.update_tray_status("In Launcher");
                    
                    let config_guard = self.config.lock().unwrap();
                    if let Err(e) = self.discord_rpc.update(&state, &config_guard) {
                        error!("Failed to update Discord RPC for Launcher: {}", e);
                    }
                }
            } else {
                // Neither running - clear presence and disconnect
                if self.discord_rpc.is_connected() {
                    let _ = self.discord_rpc.clear();
                    self.discord_rpc.disconnect();
                }
                self.update_tray_status("Waiting for Hytale...");
            }

            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }

        // Cleanup
        info!("Shutting down...");
        self.discord_rpc.disconnect();

        Ok(())
    }
}

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    info!("Hytale Discord Rich Presence v{}", env!("CARGO_PKG_VERSION"));

    let mut app = App::new()?;

    // Initialize tray (may fail on headless systems)
    if let Err(e) = app.init_tray() {
        warn!("Could not initialize tray: {}", e);
    }

    // Run main loop
    app.run()
}
