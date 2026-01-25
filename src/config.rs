//! Configuration module with platform-specific paths and constants

use std::path::PathBuf;

/// Discord Application Client ID for Hytale RPC
pub const CLIENT_ID: &str = "1461306150497550376";

/// Discord RPC asset names
pub const LARGE_IMAGE: &str = "hytale_logo";
pub const LARGE_TEXT: &str = "Hytale";

/// Polling interval in milliseconds
pub const POLL_INTERVAL_MS: u64 = 3000;

/// Process names to detect for Hytale Game Client
pub const HYTALE_GAME_PROCESSES: &[&str] = &[
    "hytale",
    "hytale.exe",
    "hytaleclient",
    "hytaleclient.exe",
];

/// Process names to detect for Hytale Launcher
pub const HYTALE_LAUNCHER_PROCESSES: &[&str] = &[
    "hytalelauncher",
    "hytalelauncher.exe",
    "hytale-launcher",
];

/// Get potential Hytale log directories based on platform
pub fn get_log_directories() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(home) = dirs::home_dir() {
        // Common path across platforms
        paths.push(home.join(".hytale/UserData/Logs"));

        #[cfg(target_os = "macos")]
        {
            if let Some(app_support) = dirs::data_dir() {
                paths.push(app_support.join("Hytale/UserData/Logs"));
            }
            paths.push(home.join("Library/Application Support/Hytale/UserData/Logs"));
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(appdata) = dirs::data_dir() {
                paths.push(appdata.join("Hytale/UserData/Logs"));
            }
            if let Some(roaming) = dirs::config_dir() {
                paths.push(roaming.join("Hytale/UserData/Logs"));
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Standard Linux paths
            paths.push(home.join(".local/share/Hytale/UserData/Logs"));
            paths.push(home.join(".config/Hytale/UserData/Logs"));

            // Flatpak paths
            paths.push(home.join(".var/app/com.hytale.Hytale/data/Hytale/UserData/Logs"));
            paths.push(home.join(".var/app/com.hytale.Hytale/config/Hytale/UserData/Logs"));

            // Steam/Proton paths
            paths.push(home.join(".steam/steam/steamapps/compatdata/Hytale/pfx/drive_c/users/steamuser/AppData/Roaming/Hytale/UserData/Logs"));
            paths.push(home.join(".local/share/Steam/steamapps/compatdata/Hytale/pfx/drive_c/users/steamuser/AppData/Roaming/Hytale/UserData/Logs"));
        }
    }

    paths
}

/// Log file pattern to search for
pub const LOG_FILE_PATTERN: &str = "*_client.log";

/// Game states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameState {
    /// In the Hytale Launcher
    Launcher,
    /// Not running or in main menu
    MainMenu,
    /// Loading a world (singleplayer or multiplayer)
    Loading {
        world_name: Option<String>,
        is_multiplayer: bool,
        sub_stage: Option<String>,
    },
    /// Playing singleplayer
    Singleplayer { world_name: String },
    /// Playing multiplayer
    Multiplayer {
        server_address: Option<String>,
        server_name: Option<String>,
    },
    /// Unknown/waiting state
    Unknown,
}

impl Default for GameState {
    fn default() -> Self {
        Self::Unknown
    }
}

impl GameState {
    /// Get Discord RPC details string
    pub fn details(&self) -> &str {
        match self {
            GameState::Launcher => "In Launcher",
            GameState::MainMenu => "In Main Menu",
            GameState::Loading { is_multiplayer, sub_stage, .. } => {
                if let Some(stage) = sub_stage {
                    return stage; // Returns &String as &str, lifetime issue? No, wait.
                    // details returns &str (static lifetime implied or match lifetime). 
                    // String is owned by the struct.
                    // We need to return Cow or just String. 
                    // But the signature is `&self -> &str`. This implies returning a reference to something in self or static.
                    // `sub_stage` is Option<String>. `sub_stage.as_str()` works.
                }
                
                if *is_multiplayer {
                    "Joining Server"
                } else {
                    "Loading World"
                }
            }
            GameState::Singleplayer { .. } => "Playing Singleplayer",
            GameState::Multiplayer { .. } => "Playing Multiplayer",
            GameState::Unknown => "Idle",
        }
    }

    /// Get Discord RPC state string
    pub fn state(&self) -> String {
        match self {
            GameState::Launcher => "Ready to Play".to_string(),
            GameState::MainMenu => "Idle".to_string(),
            GameState::Loading { world_name, sub_stage, .. } => {
                 if let Some(_) = sub_stage {
                     // If we have a sub_stage in details ("Loading..."), put world name here
                     world_name
                        .as_ref()
                        .map(|n| n.clone())
                        .unwrap_or_else(|| "Please wait...".to_string())
                 } else {
                     world_name
                        .as_ref()
                        .map(|n| n.clone())
                        .unwrap_or_else(|| "...".to_string())
                 }
            },
            GameState::Singleplayer { world_name } => format!("World: {}", world_name),
            GameState::Multiplayer {
                server_address,
                server_name,
            } => {
                if let Some(name) = server_name {
                    format!("Server: {}", name)
                } else if let Some(addr) = server_address {
                    format!("Server: {}", addr)
                } else {
                    "Online".to_string()
                }
            }
            GameState::Unknown => "Waiting...".to_string(),
        }
    }

    /// Check if currently in-game
    pub fn is_in_game(&self) -> bool {
        matches!(self, GameState::Singleplayer { .. } | GameState::Multiplayer { .. })
    }
}
