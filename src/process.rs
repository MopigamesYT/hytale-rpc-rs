//! Process detection module for Hytale and Discord

use sysinfo::System;

// Imports removed since they are used fully qualified in methods or not needed


/// Process detector for monitoring Hytale and Discord
pub struct ProcessDetector {
    system: System,
}

impl ProcessDetector {
    /// Create a new process detector
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    /// Refresh process list
    pub fn refresh(&mut self) {
        self.system.refresh_processes(sysinfo::ProcessesToUpdate::All);
    }

    /// Check if Hytale Game Client is running
    pub fn is_game_running(&self) -> bool {
        self.is_process_running(crate::config::HYTALE_GAME_PROCESSES)
    }

    /// Check if Hytale Launcher is running
    pub fn is_launcher_running(&self) -> bool {
        self.is_process_running(crate::config::HYTALE_LAUNCHER_PROCESSES)
    }

    /// Check if Discord is running
    pub fn is_discord_running(&self) -> bool {
        self.is_process_running(crate::config::DISCORD_PROCESS_NAMES)
    }

    /// Check if any of the given process names are running
    fn is_process_running(&self, names: &[&str]) -> bool {
        for process in self.system.processes().values() {
            let process_name = process.name().to_string_lossy().to_lowercase();
            for name in names {
                if process_name == name.to_lowercase()
                    || process_name.starts_with(&format!("{}.", name.to_lowercase()))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Get all running process names (for debugging)
    #[allow(dead_code)]
    pub fn get_running_processes(&self) -> Vec<String> {
        self.system
            .processes()
            .values()
            .map(|p| p.name().to_string_lossy().to_string())
            .collect()
    }
}

impl Default for ProcessDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_detector_creation() {
        let detector = ProcessDetector::new();
        // Should be able to check processes without panicking
        let _ = detector.is_discord_running();
        let _ = detector.is_game_running();
        let _ = detector.is_launcher_running();
    }
}
