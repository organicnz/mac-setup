# CLAUDE.md - Project Context for AI Assistants

## Project Overview

**mac-setup** is a macOS provisioning and auto-update system written in Rust. It is a
single native binary that provisions a fresh Mac from `packages.toml` and then keeps
it updated via a launchd agent running 3x daily.

There is no Homebrew, npm, or Ruby requirement to *run* it. It compiles to a
self-contained Mach-O binary that shells out to `stout`, `mise`, `bun`, `pipx`,
`launchctl`, `osascript`, and `xattr`.

## Architecture

```
src/
├── lib.rs                    # Re-exports the public API
├── bin/
│   └── main.rs               # CLI entry point — every subcommand is a clap variant
├── core/
│   ├── checks.rs             # Pre-flight: network, disk space
│   ├── cleanup.rs            # Cleanup engine (stout, bun, cargo, system caches)
│   ├── ops.rs                # Subprocess ops: stout/mise/pipx/bun, quarantine, timeouts
│   ├── stats.rs              # UpdateStats / CaskStats / ComponentStats + Display
│   ├── stout.rs              # The stout contract layer — every assumption about stout's output
│   └── utils.rs              # Config, cached log handles, notifications, signal handlers
└── commands/
    └── provision.rs          # One-time provisioning from packages.toml
```

## Subcommands

| Command | What it does |
|---------|--------------|
| `mac-setup provision` | Install Xcode CLT, stout, mise, then everything in `packages.toml` |
| `mac-setup update` | Pre-flight → pre-cleanup → parallel upgrades → quarantine → post-cleanup → notify |
| `mac-setup fix` | Remove known-broken casks, fix bun state, run housekeeping |
| `mac-setup audit <check> <files>` | `plist` / `secrets` / `markdown` — pre-commit checks |
| `mac-setup setup` | Install lefthook, git hooks, clippy, rustfmt |
| `mac-setup cleanup [--aggressive]` | Standalone cleanup pass |
| `mac-setup install` | Build + install the binary and launchd plist |

## The update daemon

`run_update` in `src/bin/main.rs` is the core loop. The ordering constraints that
matter:

1. **Lock first.** `/tmp/mac-setup.lock` with an exclusive flock. A contending run
   exits 0 (not a failure — launchd must not treat it as one).
2. **`stout update` first.** Every later step depends on a fresh index.
3. **Parallel fan-out.** stout formulae+casks (shared prefix, kept sequential),
   mise, pipx, and bun run on independent prefixes and execute concurrently via
   `std::thread::scope`.
4. **Quarantine after casks.** Cask upgrades mutate `/Applications`, so the
   quarantine passes run *after* the fan-out joins, never inside it.
5. **Post-cleanup.** `cleanup_after_upgrades` sweeps only what the upgrades
   refilled — `comprehensive_cleanup` already ran pre-flight.

A `SHUTDOWN` flag (set by SIGTERM/SIGINT handlers in `utils.rs`) is checked between
cask upgrades so `launchctl bootout` finishes the in-flight cask instead of dying
mid-install.

## Safety invariants

These were each real bugs. Do not reintroduce them.

- **`run_with_timeout` must drain pipes concurrently.** `wait_timeout` only polls
  `try_wait()`; a child writing past the ~64 KiB pipe buffer blocks forever and is
  then misreported as a timeout. See `ops.rs` for the threaded drain.
- **`cleanup_temp_files` must not delete the lock file.** It used to `rm -rf` anything
  >24h old in `/tmp` with no ownership check, unlinking `/tmp/mac-setup.lock` while
  the daemon held a flock on it — which silently destroyed mutual exclusion.
- **`HOME` must not be `.expect()`ed.** `Config::default()` runs for every
  subcommand, so a missing `HOME` panicked the daemon before any log line was
  written.
- **App caches must not be wiped while the app is running.** The cask upgrader
  already detects this; the cleanup engine must too.
- **Docker is not installed.** The stack is podman-only. `packages.toml` must not
  reintroduce docker/docker-desktop.

## Configuration

Runtime config via environment:

| Var | Where read | Default |
|-----|-----------|---------|
| `HOME` | `utils.rs` (required) | — |
| `MIN_DISK_SPACE_GB` | `checks.rs` | `5` |
| `TMPDIR` | `cleanup.rs` | — |
| `MAC_SETUP_HOUR{1,2,3}` / `MAC_SETUP_MINUTE{1,2,3}` | `install` only | 9/0, 15/0, 21/0 |
| `MAC_SETUP_NICE_LEVEL` | `install` only | `10` |
| `MAC_SETUP_THROTTLE_INTERVAL` | `install` only | `300` |
| `MAC_SETUP_EXIT_TIMEOUT` | `install` only | `7200` |
| `MAC_SETUP_MIN_DISK_SPACE_GB` | `install` only | `5` |
| `MAC_SETUP_PRUNE_DAYS` | `cleanup` only | `7` — stout `--prune` day count; `0` means "all" (aggressive) |
| `STOUT_PREFIX` | `stout.rs` | auto-detected — `/opt/homebrew`, then `/usr/local` |

`LOG_DIR` and `LOG_RETENTION_DAYS` are **not** consumed by the code. Log paths are
derived from `$HOME/Library/Logs`; retention is the cleanup engine's 7-day sweep.

Two placeholders in `config/com.USER.mac-setup.plist.template` are **not** substituted
by `run_install` and are not read by any code: `{{PRUNE_DAYS}}` and
`{{CACHE_MAX_AGE_DAYS}}`. The generated plist therefore contains the literal braces, and
`MAC_SETUP_PRUNE_DAYS` in the plist fails to parse and falls back to the default of 7.
The per-directory age thresholds in `cleanup.rs` (14, 30, 60 days) are hard-coded.

## Log files

```
~/Library/Logs/mac-setup.log
~/Library/Logs/mac-setup-error.log
~/Library/Logs/mac-setup-stdout.log    (launchd StandardOutPath)
~/Library/Logs/mac-setup-stderr.log    (launchd StandardErrorPath)
```

Log handles are cached process-wide (`OnceLock<Mutex<BufWriter<File>>>`) and
buffered. Re-opening per line cost thousands of `openat` syscalls per run.

## Build & test

```bash
cargo build --release
cargo test
cargo fmt --check
cargo clippy -- -D warnings
cargo run -q -- audit plist config/com.USER.mac-setup.plist.template
cargo run -q -- audit secrets src/bin/main.rs
cargo run -q -- audit markdown README.md
```

Pre-commit runs through lefthook (`lefthook run pre-commit`). The hook config
filters staged files by path inside the command — `{staged_files}` interpolates
*all* staged files, not just the glob match.

## Dependencies

| Crate | Why |
|-------|-----|
| `chrono` | Timestamp formatting |
| `clap` | CLI derive |
| `fs2` | File locking |
| `regex` | Audit pattern matching |
| `serde` + `toml` | `packages.toml` parsing |
| `signal-hook` | SIGTERM/SIGINT graceful shutdown |
| `wait-timeout` | Command timeout support |

`serde_json`, `users`, and `signal-hook` were removed as unused deps (signal-hook
was re-added when graceful shutdown was wired up).

## macOS-specific notes

- launchd, not cron. The plist template is `config/com.USER.mac-setup.plist.template`.
- Quarantine removal needs `xattr -dr com.apple.quarantine`.
- Notifications go through `osascript`. The message is escaped before interpolation
  into the AppleScript string literal.
- stout is at `~/.local/bin/stout` on Apple Silicon; mise at `~/.local/bin/mise`;
  bun at `~/.bun/bin/bun`.

## Testing

Tests live in `src/core/mod.rs` (aggregator), `src/core/ops.rs` (run_with_timeout
regressions), `src/core/stout.rs` (stout output parsers), `src/core/utils.rs`
(applescript escaping), and `src/commands/provision.rs` (manifest parsing). All are
hermetic — none spawn processes or touch the filesystem.

The key regression test is `run_with_timeout_handles_output_larger_than_pipe_buffer`:
a child writing 256 KiB must complete well under the timeout. On the pre-fix code it
hung for the full timeout and returned `Err("Timeout exceeded")`.
