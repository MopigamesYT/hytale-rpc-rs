//! Hytale Discord Rich Presence - Rust Implementation
//!
//! A system tray application that displays your Hytale game activity on Discord.

mod config;
mod log_watcher;
mod process;
mod rpc;
mod tray;

use std::thread;
use std::time::Duration;

use anyhow::Result;
use log::{error, info, warn};

use crate::config::POLL_INTERVAL_MS;
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
    hytale_was_running: bool,
    discord_was_running: bool,
}

impl App {
    fn new() -> Result<Self> {
        Ok(Self {
            process_detector: ProcessDetector::new(),
            log_watcher: LogWatcher::new(),
            discord_rpc: DiscordRpc::new(),
            tray: None,
            hytale_was_running: false,
            discord_was_running: false,
        })
    }

    fn init_tray(&mut self) -> Result<()> {
        match SystemTray::new() {
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
            println!("[Status] {}", tooltip);
        }
    }

    fn handle_tray_events(&self) -> bool {
        if let Some(ref tray) = self.tray {
            if let Some(event) = tray.poll_event() {
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

            let hytale_running = self.process_detector.is_hytale_running();
            let discord_running = self.process_detector.is_discord_running();

            // Handle Hytale state changes
            if hytale_running && !self.hytale_was_running {
                info!("Hytale detected");
                self.update_tray_status("Hytale detected");
                show_notification("Hytale RPC", "Hytale detected - monitoring game state");
            } else if !hytale_running && self.hytale_was_running {
                info!("Hytale closed");
                self.update_tray_status("Waiting for Hytale...");
                self.log_watcher.reset();
                if self.discord_rpc.is_connected() {
                    let _ = self.discord_rpc.clear();
                }
                show_notification("Hytale RPC", "Hytale closed");
            }
            self.hytale_was_running = hytale_running;

            // Handle Discord state changes
            if discord_running && !self.discord_was_running {
                info!("Discord detected");
            } else if !discord_running && self.discord_was_running {
                info!("Discord closed");
                self.discord_rpc.disconnect();
            }
            self.discord_was_running = discord_running;

            // Only monitor if both are running
            if hytale_running && discord_running {
                // Ensure connected to Discord RPC
                if !self.discord_rpc.is_connected() {
                    if let Err(e) = self.discord_rpc.connect() {
                        warn!("Could not connect to Discord RPC: {}", e);
                    }
                }

                // Update log watcher
                match self.log_watcher.update() {
                    Ok(changed) => {
                        if changed {
                            let state = self.log_watcher.state();
                            let status = format!("{} - {}", state.details(), state.state());
                            self.update_tray_status(&status);

                            // Update Discord RPC
                            if let Err(e) = self.discord_rpc.update(state) {
                                error!("Failed to update Discord RPC: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error reading log file: {}", e);
                    }
                }
            } else if hytale_running && !discord_running {
                self.update_tray_status("Waiting for Discord...");
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
