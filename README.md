# mac-setup

[![macOS](https://img.shields.io/badge/macOS-14+-blue.svg)](https://www.apple.com/macos/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://rust-lang.org)

Unified macOS package management in a single native Rust binary. Provisions a fresh Mac and keeps it updated automatically — no Homebrew, no Ruby, no Node required to run.

> ⚠️ **macOS Only**: Uses macOS-specific features (launchd, plist files, osascript notifications).

---

## Toolchain

| Tool | Role |
|------|------|
| **stout** | Package manager for formulae and casks (Rust, Homebrew-compatible) |
| **mise** | Runtime version manager — node, python, go, ruby, deno, bun, java, terraform, packer, pulumi |
| **bun** | JS runtime and package manager (replaces npm/yarn/pnpm) |
| **pipx** | Isolated Python app installer (azure-cli, etc.) |

---

## Subcommands

| Command | Description |
|---------|-------------|
| `mac-setup provision` | One-time setup: Xcode CLT → stout → mise → all packages from `packages.toml` |
| `mac-setup update` | Automated update daemon: stout + mise + bun + pipx + cleanup |
| `mac-setup install` | Install/reinstall the launchd agent (runs `update` 3× daily) |
| `mac-setup fix` | Remove broken casks, fix bun issues, housekeeping |
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

Installs Xcode CLT, stout, mise, all formulae, casks, pip packages, pipx apps, bun globals, and go tools defined in `packages.toml`.

### Install the Auto-Update Daemon

```bash
./target/release/mac-setup install
```

Runs `mac-setup update` at 9 AM, 3 PM, and 9 PM via launchd.

---

## packages.toml

All packages are defined in `packages.toml`. Edit to customize what gets provisioned.

```toml
[taps]
taps = ["supabase/tap", "hashicorp/tap", ...]

[formulae]
packages = ["git", "gh", "ffmpeg", ...]

[casks]
packages = ["ghostty", "obsidian", "figma", ...]

[mise]
tools = ["node@lts", "python@3.13", "go@latest", "bun@latest", "aqua:hashicorp/terraform@latest", ...]

[pipx]
packages = ["azure-cli"]

[pip]
packages = ["python-dotenv", "shodan", ...]

[bun]
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
                              ▼  9 AM · 3 PM · 9 PM
┌─────────────────────────────────────────────────────────────────┐
│                  ~/Scripts/mac-setup update                     │
│         (Native ARM64 Mach-O — compiled from Rust)              │
└─────────────────────────────────────────────────────────────────┘
          │               │               │               │
          ▼               ▼               ▼               ▼
    ┌──────────┐   ┌────────────┐   ┌─────────┐   ┌──────────┐
    │  stout   │   │    mise    │   │   bun   │   │  pipx    │
    │ formulae │   │  runtimes  │   │ globals │   │  apps    │
    └──────────┘   └────────────┘   └─────────┘   └──────────┘
```

---

## Update Daemon

Each run:
1. **Pre-flight**: network check, disk space check (min 5 GB)
2. **Pre-cleanup**: stout cache, bun cache, cargo cache, system caches
3. **stout update** → upgrade formulae → upgrade casks
4. **mise upgrade** — all runtimes and tools
5. **bun update --global** — bun global packages
6. **pipx upgrade-all** — Python apps
7. **Post-cleanup** + disk space report
8. **Desktop notification** on completion

---

## Logs

```bash
tail -f ~/Library/Logs/mac-setup.log
tail -f ~/Library/Logs/mac-setup-error.log
# Launchd stdout/stderr
tail -f ~/Library/Logs/mac-setup-stdout.log
tail -f ~/Library/Logs/mac-setup-stderr.log
```

---

## Launchd Management

```bash
# Status
launchctl list | grep mac-setup

# Trigger immediate run
launchctl start com.$(whoami).mac-setup

# Disable
launchctl unload ~/Library/LaunchAgents/com.$(whoami).mac-setup.plist

# Enable
launchctl load ~/Library/LaunchAgents/com.$(whoami).mac-setup.plist
```

---

## Cleanup

```bash
mac-setup cleanup                # standard
mac-setup cleanup --aggressive   # removes more cached data
```

---

## Audit (Pre-commit)

```bash
mac-setup audit plist  config/com.USER.mac-setup.plist.template
mac-setup audit secrets src/bin/main.rs
mac-setup audit markdown README.md
```

---

## Install Schedule Variables

```bash
export BREW_UPDATE_HOUR1=8    BREW_UPDATE_MINUTE1=0
export BREW_UPDATE_HOUR2=14   BREW_UPDATE_MINUTE2=30
export BREW_UPDATE_HOUR3=20   BREW_UPDATE_MINUTE3=0
mac-setup install
```

---

## Requirements

- macOS 14+ (Sonoma or later)
- Rust/Cargo (for building)
- stout and mise installed automatically by `provision`

---

## Development

```bash
cargo run --release -- setup   # install lefthook + git hooks
cargo build --release
cargo test
cargo clippy
lefthook run pre-commit
```

---

## License

MIT

---

## Changelog

### v0.3.0 — Unified Rust toolchain
- Full brew removal — stout + mise replace Homebrew
- bun replaces npm/yarn/pnpm as JS runtime and package manager
- Unified CLI: `provision`, `update`, `fix`, `audit`, `setup`, `cleanup`, `install`
- `packages.toml` manifest drives all provisioning
- launchd daemon: stout + mise + bun + pipx auto-updates 3× daily
- 25 unit tests

### v0.2.0
- Rewrite in Rust, native binary, comprehensive cleanup engine

### v0.1.0
- Initial release as `brew-auto-update`
