# Configuration Guide

`mac-setup` is configured by environment variables. There is no config file: the values
below are read at the moment the relevant subcommand runs, so anything you export in your
shell applies to the next run.

Every variable listed here is read by exactly one place in the code. If a variable is not
in this table, nothing consumes it.

---

## Runtime configuration

These are read while the daemon runs and can be set in the launchd plist's
`EnvironmentVariables` block or exported in your shell.

| Variable | Read by | Default | Meaning |
|----------|---------|---------|---------|
| `HOME` | `src/core/utils.rs` | — | **Required.** Every log path, the lock file location, and the launchd agent path are derived from it. A missing `HOME` is a hard error, never a panic. |
| `MIN_DISK_SPACE_GB` | `src/core/checks.rs` | `5` | Minimum free space on `/` before the update run starts. A non-integer value logs a warning and falls back to 5. |
| `TMPDIR` | `src/core/cleanup.rs` | system default | Directory the cleanup engine scans for abandoned temp files. Entries older than 24 hours are removed, but only when they are owned by the current user, are not symlinks, and do not hold a live socket. |
| `STOUT_PREFIX` | `src/core/stout.rs` | auto-detected | Install prefix for `stout`. When unset, the code probes `/opt/homebrew` then `/usr/local`, so Apple Silicon and Intel both work without configuration. |

### Note on `LOG_DIR` and `LOG_RETENTION_DAYS`

These are **not** read by the code. Log paths are derived from `$HOME/Library/Logs`, and
retention is enforced by the cleanup engine's own 7-day sweep over that directory. Older
revisions of this project templated them into the plist; the entries were removed because
nothing consumed them.

---

## Installation-time configuration

These are read by `mac-setup install` only. They are baked into the generated launchd
plist at install time, so export them **before** running the command.

| Variable | Default | Meaning |
|----------|---------|---------|
| `MAC_SETUP_HOUR1` / `MAC_SETUP_MINUTE1` | `9` / `0` | First daily run (09:00 by default) |
| `MAC_SETUP_HOUR2` / `MAC_SETUP_MINUTE2` | `15` / `0` | Second daily run (15:00 by default) |
| `MAC_SETUP_HOUR3` / `MAC_SETUP_MINUTE3` | `21` / `0` | Third daily run (21:00 by default) |
| `MAC_SETUP_NICE_LEVEL` | `10` | CPU priority, 0–20. Higher is lower priority. |
| `MAC_SETUP_THROTTLE_INTERVAL` | `300` | Minimum seconds between runs, in seconds |
| `MAC_SETUP_EXIT_TIMEOUT` | `7200` | Hard runtime limit for one run, in seconds |
| `MAC_SETUP_MIN_DISK_SPACE_GB` | `5` | Minimum free space written into the plist |

### Example: a custom schedule

```bash
export MAC_SETUP_HOUR1=8   MAC_SETUP_MINUTE1=0
export MAC_SETUP_HOUR2=14  MAC_SETUP_MINUTE2=30
export MAC_SETUP_HOUR3=20  MAC_SETUP_MINUTE3=0
export MAC_SETUP_NICE_LEVEL=10
export MAC_SETUP_MIN_DISK_SPACE_GB=5

mac-setup install
```

### Example: once a day instead of three times

```bash
export MAC_SETUP_HOUR1=9  MAC_SETUP_MINUTE1=0
export MAC_SETUP_HOUR2=9  MAC_SETUP_MINUTE2=0
export MAC_SETUP_HOUR3=9  MAC_SETUP_MINUTE3=0
mac-setup install
```

---

## Cleanup configuration

These are read by `mac-setup cleanup` only.

| Variable | Read by | Default | Meaning |
|----------|---------|---------|---------|
| `MAC_SETUP_PRUNE_DAYS` | `src/core/cleanup.rs` | `7` | Age in days passed to `stout --prune`. `0` means "prune everything" and is what `--aggressive` uses. A non-numeric value falls back to 7. |

The age thresholds used for the individual cache directories (14 days for the bun and
Cargo caches, 30 days for Gradle and the Swift module cache, 60 days for SwiftPM
checkouts) are currently hard-coded in `src/core/cleanup.rs`.

---

## The package manifest

`packages.toml` is the single place where the provisioned software is declared. It is not
an environment variable, but it is the other half of the configuration surface.

| Section | Consumed by | Manages |
|---------|-------------|---------|
| `[taps]` | `stout` | Third-party taps to add before installing |
| `[formulae]` | `stout` | Command-line tools and libraries |
| `[casks]` | `stout` | macOS applications |
| `[mise]` | `mise` | Language runtimes and versioned tools (node, go, deno, ruby, java, terraform, packer, pulumi) |
| `[pip]` | `pip` | Python packages installed into the system Python |
| `[pipx]` | `pipx` | Python applications installed into their own virtualenvs |
| `[bun]` | `bun` | Global JavaScript packages |
| `[go]` | `go install` | Go tools installed into `GOBIN` |

The manifest is parsed by `src/commands/provision.rs` and is the subject of the hermetic
tests in that module.

---

## Where the configuration lands

`mac-setup install` renders `config/com.USER.mac-setup.plist.template` into
`~/Library/LaunchAgents/com.$USER.mac-setup.plist`, substituting the placeholders in the
template with the values above.

To change the schedule of an already-installed agent, either re-run `mac-setup install`
with the new environment, or edit the plist by hand and reload it:

```bash
launchctl unload ~/Library/LaunchAgents/com.$(whoami).mac-setup.plist
launchctl load   ~/Library/LaunchAgents/com.$(whoami).mac-setup.plist
```

To inspect what is actually installed:

```bash
launchctl print gui/$(id -u)/com.$(whoami).mac-setup
plutil -p ~/Library/LaunchAgents/com.$(whoami).mac-setup.plist
```

---

## Logs

The cleanup engine does not rotate logs, so the 7-day sweep in `mac-setup cleanup` is what
keeps them bounded.

```
~/Library/Logs/mac-setup.log           # The main log
~/Library/Logs/mac-setup-error.log     # Errors only
~/Library/Logs/mac-setup-stdout.log    # launchd StandardOutPath
~/Library/Logs/mac-setup-stderr.log    # launchd StandardErrorPath
```
