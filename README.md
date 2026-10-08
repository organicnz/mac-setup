# mac-setup

[![macOS](https://img.shields.io/badge/macOS-10.14+-blue.svg)](https://www.apple.com/macos/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://rust-lang.org)

Unified macOS package management in a single native Rust binary.  
One tool to provision a fresh Mac and keep it updated automatically.

> ⚠️ **macOS Only**: Uses macOS-specific features (launchd, plist files, osascript notifications).

---

## Subcommands

| Command | Description |
|---------|-------------|
| `mac-setup provision` | One-time setup: Xcode CLT → Homebrew → all packages from `packages.toml` |
| `mac-setup update` | Automated update daemon: brew + casks + npm + cleanup |
| `mac-setup install` | Install/reinstall the launchd agent (runs `update` 3x daily) |
| `mac-setup fix` | Remove broken casks, fix npm issues, housekeeping |
| `mac-setup audit` | Pre-commit checks: plist validation, secrets scanning, markdown linting |
| `mac-setup setup` | Dev environment setup: lefthook, clippy, rustfmt, git hooks |
| `mac-setup cleanup` | Run comprehensive system cleanup standalone |

---

## Quick Start

### Fresh Mac Setup

```bash
git clone https://github.com/organicnz/mac-setup.git
cd mac-setup
cargo build --release
./target/release/mac-setup provision
```

This installs Xcode CLT, Homebrew, all formulae, casks, pip packages, npm globals, and go tools
defined in `packages.toml`.

### Install the Auto-Update Daemon

```bash
./target/release/mac-setup install
```

The daemon runs `mac-setup update` at 9 AM, 3 PM, and 9 PM via launchd.

---

## packages.toml

All installed packages are defined in `packages.toml` at the repo root. Edit it to customize
what gets provisioned on a new machine.

```toml
[taps]
taps = ["homebrew/core", "hashicorp/tap", ...]

[formulae]
packages = ["git", "wget", "terraform", ...]

[casks]
packages = ["docker", "firefox", "slack", ...]

[pip]
packages = ["python-dotenv", "shodan", ...]

[npm]
global = ["imgproxy"]

[go]
packages = ["github.com/tomnomnom/httprobe@master"]
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         macOS launchd                           │
│  (reads ~/Library/LaunchAgents/com.USER.mac-setup.plist)        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼  Runs at 9 AM, 3 PM, 9 PM
┌─────────────────────────────────────────────────────────────────┐
│                  ~/Scripts/mac-setup update                     │
│              (Native ARM64/x86 Mach-O binary)                   │
└─────────────────────────────────────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
    ┌──────────┐       ┌──────────────┐    ┌───────────┐
    │ Pre-flight│       │ brew update  │    │   Logs    │
    │  Checks   │       │ brew upgrade │    │ & Notify  │
    └──────────┘       └──────────────┘    └───────────┘
```

---

## Update Daemon Features

- **Pre-flight checks**: network connectivity, disk space (min 5GB)
- **Lock file**: prevents concurrent runs
- **Homebrew**: `brew update` → upgrade formulae → upgrade casks (with timeout/recovery)
- **Quarantine removal**: strips `com.apple.quarantine` from all cask apps and formula binaries
- **NPM**: updates all global npm packages, detects invalid package names
- **Comprehensive cleanup**: brew cache, npm/cargo/system caches, browser caches, Xcode, temp files
- **Desktop notifications**: success/warning/skip via osascript
- **Logging**: timestamped, auto-rotated, separate error log

---

## Logs

```bash
tail -f ~/Library/Logs/brew-updates.log
tail -f ~/Library/Logs/brew-updates-error.log
```

---

## Launchd Management

```bash
# Check status
launchctl list | grep mac-setup

# Trigger immediate run
launchctl start com.$(whoami).mac-setup

# Disable
launchctl unload ~/Library/LaunchAgents/com.$(whoami).mac-setup.plist

# Enable
launchctl load ~/Library/LaunchAgents/com.$(whoami).mac-setup.plist
```

---

## Cleanup Options

```bash
# Standard cleanup
mac-setup cleanup

# Aggressive (removes more cached data)
mac-setup cleanup --aggressive
```

---

## Audit (Pre-commit Hooks)

```bash
mac-setup audit plist config/com.USER.brew-update.plist.template
mac-setup audit secrets src/bin/main.rs
mac-setup audit markdown README.md
```

---

## Installation Variables

Customize the launchd schedule before running `mac-setup install`:

```bash
export BREW_UPDATE_HOUR1=8    BREW_UPDATE_MINUTE1=0
export BREW_UPDATE_HOUR2=14   BREW_UPDATE_MINUTE2=30
export BREW_UPDATE_HOUR3=20   BREW_UPDATE_MINUTE3=0
export BREW_UPDATE_NICE_LEVEL=10
export BREW_UPDATE_MIN_DISK_SPACE_GB=5
mac-setup install
```

---

## Requirements

- macOS 10.14+ (Mojave or later)
- Rust/Cargo (for building)
- Homebrew (installed automatically by `provision`)

---

## Development

```bash
# Setup dev tools
cargo run --release -- setup

# Build
cargo build --release

# Lint
cargo clippy

# Run pre-commit checks
lefthook run pre-commit
```

---

## License

MIT

---

## Changelog

### v0.3.0 — Unified CLI
- Merged `brew-installer` (bash) and `brew-auto-update` (Rust daemon) into a single binary
- New `provision` subcommand replaces the bash installer script
- `packages.toml` manifest for all package definitions
- All commands unified under `mac-setup <subcommand>` with clap

### v0.2.0
- Rewrite in Rust, native binary
- Comprehensive cleanup engine
- NPM integration

### v0.1.0
- Initial release as `brew-auto-update`
