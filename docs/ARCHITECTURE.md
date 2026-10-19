# Architecture

`mac-setup` is a single native binary that does two jobs: it provisions a Mac from a
declarative manifest, and it keeps that Mac updated through a launchd agent.

There is no Homebrew, no npm, and no Ruby in the runtime path. The binary shells out to
`stout` (packages), `mise` (runtimes and versioned tools), `bun` (JavaScript runtime and
package manager), and `pipx` (isolated Python applications).

---

## The shape of the system

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
     │ + casks  │   │ + tools    │   │         │   │          │
     └──────────┘   └────────────┘   └─────────┘   └──────────┘
```

---

## The update daemon

`run_update` in `src/bin/main.rs` is the core loop. The ordering constraints below are
load-bearing — each of them exists because the order in the other direction broke
something.

1. **Take the lock first.** `/tmp/mac-setup.lock` with an exclusive `flock`. A contending
   run exits **0**, not 1: launchd must not treat a skipped run as a failure.
2. **Pre-flight.** Network reachability, then disk space. If the disk is below
   `MIN_DISK_SPACE_GB`, an aggressive cleanup runs and the check is repeated.
3. **Pre-cleanup.** The comprehensive sweep runs before anything is upgraded, so the
   upgrade has room to work.
4. **`stout update` first, on its own.** Every later step depends on a fresh package
   index.
5. **Parallel fan-out.** `stout` (formulae then casks, sequential — they share a prefix
   and stout's own locks), `mise`, `pipx`, and `bun` run on independent prefixes and
   execute concurrently through `std::thread::scope`. A panicking worker is treated as a
   failed upgrade, not a dead daemon.
6. **Quarantine after the fan-out joins.** Cask upgrades mutate `/Applications`, so the
   `xattr -dr com.apple.quarantine` passes run *after* the threads finish, never inside
   them.
7. **Shutdown check.** A `SHUTDOWN` flag — set by the SIGTERM/SIGINT handlers in
   `src/core/utils.rs` — is checked between cask upgrades so `launchctl bootout` lets the
   in-flight cask finish instead of dying mid-install.
8. **Post-cleanup.** `cleanup_after_upgrades` sweeps only what the upgrades refilled. The
   comprehensive sweep already ran in step 3, so running it again would double the cost
   of every run for no benefit.
9. **Summary, disk report, and a desktop notification.**

### Exit codes

| Situation | Exit code |
|-----------|-----------|
| Everything updated | `0` |
| Updated, but some items need a human (a cask blocked on an auth prompt, a formula with no bottle) | `0`, reported as "some items require attention" |
| An upgrade phase failed, a worker panicked, or stout's output could not be parsed | `1` |
| Another instance held the lock | `0` |
| No network, or not enough disk space even after aggressive cleanup | `1` |

Drift in stout's output is a hard failure on purpose: if the parsers in
`src/core/stout.rs` no longer understand what stout printed, the run cannot confirm it
did anything, and reporting success would be a lie.

---

## The stout contract layer

`src/core/stout.rs` is the only place that knows what `stout` looks like. Three separate
bugs came from assuming it behaves like Homebrew:

- It has **no `--json` flag**. The old code called `stout info --cask --json=v2`, which
  always exited 2, so cask quarantine was never stripped.
- It prints **`All packages are up to date.` to stdout** on exit 0. The old code counted
  that as a package to upgrade — and tried to upgrade the phantom first word, `All`.
- It **exits 0 even when a batch upgrade fails**. Failures have to be read out of the
  output, not out of the exit status.

Every parser in that module is a pure function over captured stdout, so they are covered
by hermetic tests against real stout 0.2.2 output. When stout's output changes, that is
the file to change.

---

## The cleanup engine

`src/core/cleanup.rs` sweeps the caches that accumulate on a long-lived Mac: the stout
cache, the bun cache, the Cargo registry and target caches, Xcode archives and derived
data, Gradle, CocoaPods, Go, the Swift module cache, SwiftPM checkouts, and the system
caches.

Two rules keep it from doing damage:

- **Ownership.** Entries in the shared temp directory are only removed when they are owned
  by the current user, are not symlinks, and do not hold a live socket. The old version
  deleted anything older than 24 hours, which included other users' files and — worse —
  `mac-setup.lock` itself, silently destroying mutual exclusion.
- **Running apps.** Application caches are not wiped while the application is running.

`mac-setup cleanup --dry-run` reports what would be freed and deletes nothing.
`mac-setup cleanup --aggressive` lowers the age thresholds and prunes the stout cache
entirely.

---

## Provisioning

`src/commands/provision.rs` reads `packages.toml` and installs it in dependency order:
Xcode Command Line Tools first, then `stout`, then `mise`, then the taps, then the
formulae, casks, mise tools, pip packages, pipx applications, bun globals, and Go tools.

The manifest is parsed into a typed structure and the parsing is unit-tested, so a typo in
`packages.toml` fails at parse time rather than halfway through a 20-minute install.

---

## Logging

Four files under `~/Library/Logs`:

```
mac-setup.log           # The main log
mac-setup-error.log     # Errors only
mac-setup-stdout.log    # launchd StandardOutPath
mac-setup-stderr.log    # launchd StandardErrorPath
```

Log handles are cached process-wide behind a `OnceLock<Mutex<BufWriter<File>>>` and
buffered. Re-opening the file per line cost thousands of `openat` syscalls per run.

There is no log rotation. The cleanup engine's 7-day sweep over `~/Library/Logs` is what
keeps the files bounded.

---

## Notifications

Completion is reported with `osascript`. The message is escaped before it is interpolated
into the AppleScript string literal, so a cask or package name containing a quote cannot
break out of the string.

---

## The launchd agent

`config/com.USER.mac-setup.plist.template` is rendered at install time into
`~/Library/LaunchAgents/com.$USER.mac-setup.plist`. The placeholders in the template are
substituted with the username, the home directory, the schedule, and the tunables
described in [CONFIGURATION.md](CONFIGURATION.md).

The agent runs at `ProcessType Background`, with `LowPriorityIO`, a `Nice` level, a
`ThrottleInterval`, and an `ExitTimeOut`, so a background update never competes with the
work in front of the user.

---

## Module map

| Module | Responsibility |
|--------|----------------|
| `src/bin/main.rs` | CLI surface (clap), the update daemon, `install`, `audit`, `fix` |
| `src/commands/provision.rs` | Manifest parsing and one-time provisioning |
| `src/core/checks.rs` | Network and disk-space pre-flight |
| `src/core/cleanup.rs` | The cleanup engine and its safety rules |
| `src/core/ops.rs` | Subprocess execution, timeouts, quarantine removal |
| `src/core/stout.rs` | The `stout` contract layer and its parsers |
| `src/core/stats.rs` | Upgrade statistics and their `Display` output |
| `src/core/utils.rs` | Config, cached log handles, notifications, signal handlers |
