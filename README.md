# Hytale Discord Rich Presence (Rust)

A lightweight system tray application that displays your Hytale game activity on Discord, written in Rust.

## Features

- **Automatic Detection**: Monitors Hytale and Discord processes automatically
- **Game State Tracking**: Detects and displays:
  - Main Menu (Idle)
  - Loading World
  - Joining Server
  - Playing Singleplayer (with world name)
  - Playing Multiplayer (with server address)
- **Play Time Tracking**: Shows elapsed time while in-game
- **System Tray**: Cross-platform tray icon with status display
- **Notifications**: System notifications for state changes
- **Cross-Platform**: Works on Windows, macOS, and Linux

## Installation

### Pre-built Binaries

Download the latest release for your platform from the [Releases](https://github.com/MopigamesYT/hytale-rpc-rs/releases) page.

### Building from Source

Requirements:
- Rust 1.70 or later
- Platform-specific dependencies (see below)

```bash
# Clone the repository
git clone https://github.com/MopigamesYT/hytale-rpc-rs
cd hytale-rpc-rs

# Build release binary
cargo build --release

# The binary will be at target/release/hytale-rpc
```

#### Linux Dependencies

```bash
# Debian/Ubuntu
sudo apt-get install libdbus-1-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev

# Fedora
sudo dnf install dbus-devel gtk3-devel libappindicator-gtk3-devel libxdo-devel

# Arch
sudo pacman -S dbus gtk3 libappindicator-gtk3 xdotool
```

#### macOS Dependencies

No additional dependencies required.

#### Windows Dependencies

No additional dependencies required.

## Usage

1. Start the application
2. Launch Hytale
3. Your Discord status will automatically update

The application runs in the system tray and displays:
- Current game state
- World/server name
- Play time

### Command Line Options

```bash
# Run with debug logging
RUST_LOG=debug ./hytale-rpc

# Run with trace logging (very verbose)
RUST_LOG=trace ./hytale-rpc
```

## Configuration

The application automatically detects Hytale log files in the following locations:

### Windows
- `%APPDATA%/Hytale/UserData/Logs`
- `~/.hytale/UserData/Logs`

### macOS
- `~/Library/Application Support/Hytale/UserData/Logs`
- `~/.hytale/UserData/Logs`

### Linux
- `~/.hytale/UserData/Logs`
- `~/.local/share/Hytale/UserData/Logs`
- `~/.config/Hytale/UserData/Logs`
- Flatpak and Steam/Proton paths are also supported

## How It Works

1. **Process Detection**: Monitors running processes for Hytale and Discord
2. **Log Parsing**: Reads Hytale client logs to detect game state changes
3. **Discord RPC**: Sends activity updates to Discord via IPC

## License

MIT License - see [LICENSE](LICENSE) for details.

## Credits

- Original Python implementation: [hytale-rpc](https://github.com/bas3line/hytale-rpc) by bas3line
- Discord Rich Presence: [discord-rich-presence](https://crates.io/crates/discord-rich-presence)
