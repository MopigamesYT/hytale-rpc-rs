//! Log watcher and parser for Hytale client logs

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result};
use log::{debug, info};
use regex::Regex;

use crate::config::{get_log_directories, GameState, LOG_FILE_PATTERN};

/// Log patterns for detecting game state
pub struct LogPatterns {
    main_menu: Regex,
    singleplayer_world: Regex,
    singleplayer_create: Regex,
    multiplayer_connect: Regex,
    server_connect: Regex,
    in_game: Regex,
    world_loaded: Regex,
    server_name: Regex,
    playing_singleplayer: Regex,
    playing_multiplayer: Regex,
    loading_stage: Regex,
}

impl LogPatterns {
    pub fn new() -> Self {
        Self {
            main_menu: Regex::new(
                r"Changing Stage to MainMenu|Changing from Stage (?:Loading|GameLoading|Startup) to MainMenu",
            )
            .unwrap(),
            singleplayer_world: Regex::new(r#"Connecting to singleplayer world "([^"]+)""#)
                .unwrap(),
            singleplayer_create: Regex::new(r"Creating new singleplayer world in|Creating world")
                .unwrap(),
            multiplayer_connect: Regex::new(
                r"Connecting to (?:multiplayer|dedicated) server|Server connection established",
            )
            .unwrap(),
            server_connect: Regex::new(r"Opening Quic Connection to ([\d\w\.-]+):(\d+)").unwrap(),
            in_game: Regex::new(
                r"Changing from Stage (?:GameLoading|Loading) to InGame|GameInstance\.StartJoiningWorld|GameInstance\.OnWorldJoined",
            )
            .unwrap(),
            world_loaded: Regex::new(
                r"World loaded|World finished loading|World ready|Loading world:",
            )
            .unwrap(),
            server_name: Regex::new(r#"Server name:?\s*"([^"]+)"|Joined server:?\s*"([^"]+)""#)
                .unwrap(),
            playing_singleplayer: Regex::new(
                r#"Singleplayer world "([^"]+)"|Playing in singleplayer|Singleplayer mode"#,
            )
            .unwrap(),
            playing_multiplayer: Regex::new(
                r"Playing in multiplayer|Multiplayer mode|Multi player|dedicated server",
            )
            .unwrap(),
            loading_stage: Regex::new(r"Changing from loading stage (\w+) to (\w+)").unwrap(),
        }
    }
}

impl Default for LogPatterns {
    fn default() -> Self {
        Self::new()
    }
}

/// Log watcher for monitoring Hytale client logs
pub struct LogWatcher {
    patterns: LogPatterns,
    current_log_path: Option<PathBuf>,
    file_position: u64,
    current_state: GameState,
    pending_world_name: Option<String>,
    pending_server_address: Option<String>,
    pending_server_name: Option<String>,
    is_multiplayer: bool,
}

impl LogWatcher {
    /// Create a new log watcher
    pub fn new() -> Self {
        Self {
            patterns: LogPatterns::new(),
            current_log_path: None,
            file_position: 0,
            current_state: GameState::Unknown,
            pending_world_name: None,
            pending_server_address: None,
            pending_server_name: None,
            is_multiplayer: false,
        }
    }

    /// Reset the watcher state
    pub fn reset(&mut self) {
        self.current_log_path = None;
        self.file_position = 0;
        self.current_state = GameState::Unknown;
        self.pending_world_name = None;
        self.pending_server_address = None;
        self.pending_server_name = None;
        self.is_multiplayer = false;
    }

    /// Get current game state
    pub fn state(&self) -> &GameState {
        &self.current_state
    }

    /// Find the most recent log file
    fn find_latest_log_file(&self) -> Option<PathBuf> {
        let log_dirs = get_log_directories();
        let mut latest_file: Option<(PathBuf, SystemTime)> = None;

        for dir in log_dirs {
            if !dir.exists() {
                continue;
            }

            // Convert glob pattern to regex-like matching
            let pattern = LOG_FILE_PATTERN.replace("*", "");

            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if filename.ends_with(&pattern) {
                            if let Ok(metadata) = entry.metadata() {
                                if let Ok(modified) = metadata.modified() {
                                    match &latest_file {
                                        None => latest_file = Some((path, modified)),
                                        Some((_, latest_time)) if modified > *latest_time => {
                                            latest_file = Some((path, modified));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        latest_file.map(|(path, _)| path)
    }

    /// Update the log watcher, reading new lines and updating state
    pub fn update(&mut self) -> Result<bool> {
        // Find latest log file if we don't have one or it changed
        let latest_log = self.find_latest_log_file();

        if latest_log != self.current_log_path {
            if let Some(ref path) = latest_log {
                info!("Found log file: {}", path.display());
            }
            self.current_log_path = latest_log;
            self.file_position = 0;
        }

        let log_path = match &self.current_log_path {
            Some(path) => path.clone(),
            None => {
                debug!("No log file found");
                return Ok(false);
            }
        };

        // Open and read new lines
        let file = File::open(&log_path).context("Failed to open log file")?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        // Check if file was truncated (new session)
        if file_size < self.file_position {
            info!("Log file was truncated, resetting position");
            self.file_position = 0;
            self.current_state = GameState::Unknown;
        }

        // No new content
        if file_size == self.file_position {
            return Ok(false);
        }

        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(self.file_position))?;

        let mut state_changed = false;
        let mut line = String::new();

        while reader.read_line(&mut line)? > 0 {
            if self.parse_line(&line) {
                state_changed = true;
            }
            line.clear();
        }

        self.file_position = reader.stream_position()?;

        Ok(state_changed)
    }

    /// Parse a single log line and update state
    fn parse_line(&mut self, raw_line: &str) -> bool {
        let raw_line = raw_line.trim();
        if raw_line.is_empty() {
            return false;
        }

        // Try to extract message from pipe-delimited format
        // Format: Timestamp|Level|Source|Message
        // If it matches this format, use the message part. Otherwise use the whole line.
        let parts: Vec<&str> = raw_line.splitn(4, '|').collect();
        let line = if parts.len() == 4 {
            parts[3].trim()
        } else {
            raw_line
        };

        // Check for main menu
        if self.patterns.main_menu.is_match(line) {
            debug!("Detected: Main Menu");
            self.current_state = GameState::MainMenu;
            self.pending_world_name = None;
            self.pending_server_address = None;
            self.pending_server_name = None;
            self.is_multiplayer = false;
            return true;
        }

        // Check for singleplayer world connection
        if let Some(caps) = self.patterns.singleplayer_world.captures(line) {
            if let Some(world_name) = caps.get(1) {
                let name = world_name.as_str().to_string();
                debug!("Detected: Connecting to singleplayer world '{}'", name);
                self.pending_world_name = Some(name.clone());
                self.is_multiplayer = false;
                self.current_state = GameState::Loading {
                    world_name: Some(name),
                    is_multiplayer: false,
                    sub_stage: None,
                };
                return true;
            }
        }

        // Check for singleplayer world creation
        if self.patterns.singleplayer_create.is_match(line) {
            debug!("Detected: Creating singleplayer world");
            self.is_multiplayer = false;
            self.current_state = GameState::Loading {
                world_name: self.pending_world_name.clone(),
                is_multiplayer: false,
                sub_stage: None,
            };
            return true;
        }

        // Check for multiplayer connection
        if self.patterns.multiplayer_connect.is_match(line) {
            debug!("Detected: Multiplayer connection");
            self.is_multiplayer = true;
            self.current_state = GameState::Loading {
                world_name: None,
                is_multiplayer: true,
                sub_stage: None,
            };
            return true;
        }

        // Check for loading stages
        if let Some(caps) = self.patterns.loading_stage.captures(line) {
            if let Some(stage) = caps.get(2) {
                let stage_name = stage.as_str();
                debug!("Detected: Loading stage '{}'", stage_name);
                
                // Only update if we are already in loading state or about to be
                if let GameState::Loading { world_name, is_multiplayer, .. } = &self.current_state {
                    // Convert CamelCase to Spaced String (e.g. BootingServer -> Booting Server)
                    let formatted_stage = self.format_stage_name(stage_name);
                    self.current_state = GameState::Loading {
                        world_name: world_name.clone(),
                        is_multiplayer: *is_multiplayer,
                        sub_stage: Some(format!("Loading: {}", formatted_stage)),
                    };
                    return true;
                }
            }
        }

        // Check for server address
        if let Some(caps) = self.patterns.server_connect.captures(line) {
            if let (Some(host), Some(port)) = (caps.get(1), caps.get(2)) {
                let host_str = host.as_str();
                let address = format!("{}:{}", host_str, port.as_str());
                debug!("Detected: Server address {}", address);

                // Check if it's localhost - treat as singleplayer
                let is_localhost = host_str == "127.0.0.1"
                    || host_str == "localhost"
                    || host_str == "::1";

                if is_localhost {
                    debug!("Localhost detected, treating as singleplayer");
                    self.is_multiplayer = false;
                } else {
                    self.pending_server_address = Some(address);
                    self.is_multiplayer = true;
                }
                return false; // Don't trigger state change yet
            }
        }

        // Check for server name
        if let Some(caps) = self.patterns.server_name.captures(line) {
            let name = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str().to_string());
            if let Some(ref n) = name {
                debug!("Detected: Server name '{}'", n);
            }
            self.pending_server_name = name;
            return false; // Don't trigger state change yet
        }

        // Check for in-game transition
        if self.patterns.in_game.is_match(line) || self.patterns.world_loaded.is_match(line) {
            debug!("Detected: In-game / World loaded");
            if self.is_multiplayer {
                self.current_state = GameState::Multiplayer {
                    server_address: self.pending_server_address.clone(),
                    server_name: self.pending_server_name.clone(),
                };
            } else {
                self.current_state = GameState::Singleplayer {
                    world_name: self
                        .pending_world_name
                        .clone()
                        .unwrap_or_else(|| "Exploring Orbis".to_string()),
                };
            }
            return true;
        }

        // Check for playing singleplayer indicators
        if let Some(caps) = self.patterns.playing_singleplayer.captures(line) {
            if let Some(world_name) = caps.get(1) {
                let name = world_name.as_str().to_string();
                debug!("Detected: Playing singleplayer '{}'", name);
                self.current_state = GameState::Singleplayer { world_name: name };
                return true;
            }
        }

        // Check for playing multiplayer indicators
        if self.patterns.playing_multiplayer.is_match(line) {
            debug!("Detected: Playing multiplayer");
            if !matches!(self.current_state, GameState::Multiplayer { .. }) {
                self.current_state = GameState::Multiplayer {
                    server_address: self.pending_server_address.clone(),
                    server_name: self.pending_server_name.clone(),
                };
                return true;
            }
        }

        false
    }

    /// Helper to format stage names (e.g. "BootingServer" -> "Booting Server")
    fn format_stage_name(&self, stage: &str) -> String {
        let mut result = String::new();
        for (i, c) in stage.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                result.push(' ');
            }
            result.push(c);
        }
        result
    }
}

impl Default for LogWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_patterns() {
        let patterns = LogPatterns::new();

        assert!(patterns.main_menu.is_match("Changing Stage to MainMenu"));
        assert!(patterns
            .main_menu
            .is_match("Changing from Stage Loading to MainMenu"));

        let caps = patterns
            .singleplayer_world
            .captures(r#"Connecting to singleplayer world "TestWorld""#);
        assert!(caps.is_some());
        assert_eq!(caps.unwrap().get(1).unwrap().as_str(), "TestWorld");

        assert!(patterns
            .server_connect
            .is_match("Opening Quic Connection to play.hytale.com:25565"));
    }

    #[test]
    fn test_log_watcher_creation() {
        let watcher = LogWatcher::new();
        assert!(matches!(watcher.state(), GameState::Unknown));
    }

    #[test]
    fn test_new_log_format() {
        let mut watcher = LogWatcher::new();
        // Test Main Menu detection with new format
        let line = "2026-01-25 11:06:22.6288|INFO|HytaleClient.Application.Program|Changing from Stage Startup to MainMenu";
        assert!(watcher.parse_line(line));
        assert!(matches!(watcher.state(), GameState::MainMenu));
    }

    #[test]
    fn test_loading_stages() {
        let mut watcher = LogWatcher::new();
        
        // First simulate entering loading state
        let connect_line = r#"2026-01-25 11:16:40.2349|INFO|HytaleClient.Application.AppStartup|Connecting to singleplayer world "TestWorld"..."#;
        assert!(watcher.parse_line(connect_line));
        
        if let GameState::Loading { world_name, sub_stage, .. } = watcher.state() {
            assert_eq!(world_name.as_deref(), Some("TestWorld"));
            assert!(sub_stage.is_none());
        } else {
            panic!("State should be Loading");
        }

        // Test detailed stage update
        let stage_line = "2026-01-25 11:16:40.5987|INFO|HytaleClient.Application.AppMainMenu|Changing from loading stage Initial to BootingServer";
        assert!(watcher.parse_line(stage_line));

        if let GameState::Loading { sub_stage, .. } = watcher.state() {
            assert_eq!(sub_stage.as_deref(), Some("Loading: Booting Server"));
        } else {
            panic!("State should be Loading");
        }
    }
}
