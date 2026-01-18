//! Discord Rich Presence module

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use log::{debug, error, info, warn};

use crate::config::{GameState, CLIENT_ID, LARGE_IMAGE, LARGE_TEXT};

/// Discord RPC manager
pub struct DiscordRpc {
    client: Option<DiscordIpcClient>,
    connected: bool,
    start_timestamp: Option<i64>,
    last_state: Option<GameState>,
}

impl DiscordRpc {
    /// Create a new Discord RPC manager
    pub fn new() -> Self {
        Self {
            client: None,
            connected: false,
            start_timestamp: None,
            last_state: None,
        }
    }

    /// Check if connected to Discord
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Connect to Discord RPC
    pub fn connect(&mut self) -> Result<()> {
        if self.connected {
            return Ok(());
        }

        info!("Connecting to Discord RPC...");

        let mut client = DiscordIpcClient::new(CLIENT_ID)
            .map_err(|e| anyhow::anyhow!("Failed to create Discord IPC client: {}", e))?;

        match client.connect() {
            Ok(_) => {
                info!("Connected to Discord RPC");
                self.client = Some(client);
                self.connected = true;
                Ok(())
            }
            Err(e) => {
                warn!("Failed to connect to Discord RPC: {}", e);
                Err(anyhow::anyhow!("Failed to connect to Discord: {}", e))
            }
        }
    }

    /// Disconnect from Discord RPC
    pub fn disconnect(&mut self) {
        if let Some(ref mut client) = self.client {
            if let Err(e) = client.close() {
                error!("Error closing Discord RPC: {}", e);
            }
        }
        self.client = None;
        self.connected = false;
        self.start_timestamp = None;
        self.last_state = None;
        info!("Disconnected from Discord RPC");
    }

    /// Clear the Discord presence
    pub fn clear(&mut self) -> Result<()> {
        if let Some(ref mut client) = self.client {
            client.clear_activity()
                .map_err(|e| anyhow::anyhow!("Failed to clear activity: {}", e))?;
            self.last_state = None;
            debug!("Cleared Discord presence");
        }
        Ok(())
    }

    /// Update Discord presence with the current game state
    pub fn update(&mut self, state: &GameState) -> Result<()> {
        // Skip update if state hasn't changed
        if self.last_state.as_ref() == Some(state) {
            return Ok(());
        }

        let client = match self.client.as_mut() {
            Some(c) => c,
            None => {
                self.connect()?;
                self.client.as_mut().unwrap()
            }
        };

        // Set start timestamp when entering game
        if state.is_in_game() && !self.last_state.as_ref().map(|s| s.is_in_game()).unwrap_or(false) {
            self.start_timestamp = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            );
        }

        // Clear timestamp when leaving game
        if !state.is_in_game() {
            self.start_timestamp = None;
        }

        let details = state.details();
        let state_str = state.state();

        debug!("Updating Discord presence: {} - {}", details, state_str);

        // Build activity
        let mut activity_builder = activity::Activity::new()
            .details(details)
            .state(&state_str)
            .assets(
                activity::Assets::new()
                    .large_image(LARGE_IMAGE)
                    .large_text(LARGE_TEXT),
            )
            .buttons(vec![activity::Button::new(
                "Hytale Website",
                "https://hytale.com",
            )]);

        // Add timestamp if in-game
        if let Some(timestamp) = self.start_timestamp {
            activity_builder = activity_builder.timestamps(activity::Timestamps::new().start(timestamp));
        }

        match client.set_activity(activity_builder) {
            Ok(_) => {
                self.last_state = Some(state.clone());
                debug!("Discord presence updated successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to update Discord presence: {}", e);
                // Try to reconnect on error
                self.connected = false;
                self.client = None;
                Err(anyhow::anyhow!("Failed to update presence: {}", e))
            }
        }
    }
}

impl Default for DiscordRpc {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DiscordRpc {
    fn drop(&mut self) {
        self.disconnect();
    }
}
