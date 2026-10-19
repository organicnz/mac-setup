# Contributing to mac-setup

Thanks for taking a look. `mac-setup` is a single native Rust binary that provisions a
fresh Mac from `packages.toml` and then keeps it updated through a launchd agent. It is
macOS-only, and it is deliberately small: one binary, one manifest, one scheduler.

This document explains how to get set up, what we expect from a pull request, and which
parts of the codebase are load-bearing.

---

## Table of contents

- [Ways to contribute](#ways-to-contribute)
- [Getting started](#getting-started)
- [Project layout](#project-layout)
- [Before you open a pull request](#before-you-open-a-pull-request)
- [Coding conventions](#coding-conventions)
- [Safety invariants](#safety-invariants)
- [Testing](#testing)
- [Commit and branch conventions](#commit-and-branch-conventions)
- [Opening issues](#opening-issues)
- [Community expectations](#community-expectations)
- [License](#license)

---

## Ways to contribute

Not every contribution has to be Rust.

| Contribution | Where to start | Notes |
|--------------|----------------|-------|
| Report a bug | [Open an issue](https://github.com/organicnz/mac-setup/issues/new?template=bug_report.yml) | Include the output of `mac-setup --version` and the relevant log tail |
| Propose a feature | [Open an issue](https://github.com/organicnz/mac-setup/issues/new?template=feature_request.yml) | Describe the workflow it replaces, not just the feature |
| Work on a labelled issue | Issues labelled `good first issue` or `help wanted` | Comment on the issue before you start so two people do not duplicate work |
| Add tests | `src/core/ops.rs`, `src/core/utils.rs`, `src/commands/provision.rs` | Tests must stay hermetic — no process spawning, no filesystem writes |
| Improve the docs | `README.md`, `docs/`, `CLAUDE.md` | The docs describe the *current* system; stale docs about Homebrew are considered bugs |
| Improve `packages.toml` | `packages.toml` | Add a package, and say in the PR why it belongs in the manifest |

If you are new to the project, the `good first issue` label is the intended on-ramp. Each
of those issues is scoped so that a single pull request can close it.

---

## Getting started

### Prerequisites

- macOS 14 (Sonoma) or later
- Rust via [rustup](https://rustup.rs) — the pinned toolchain is in `rust-toolchain.toml`
- `lefthook` for the git hooks (optional but recommended)

### Fork and clone

```bash
git clone https://github.com/<your-username>/mac-setup.git
cd mac-setup
```

### Build and test

```bash
cargo build --release
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

### Install the git hooks

```bash
cargo run --release -- setup
```

This installs `lefthook`, the pre-commit and pre-push hooks, and the clippy/rustfmt
toolchain checks. The pre-commit hook runs `cargo fmt --check`, `cargo clippy -- -D warnings`,
plist template validation, a secrets scan, and markdown linting over the staged files.

### Try it without installing

You can run every subcommand straight from the working tree. Nothing is written outside
`$HOME/Library/Logs` and `$HOME/Library/LaunchAgents`, and `mac-setup cleanup --dry-run`
reports what it would free without deleting anything.

---

## Project layout

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
│   ├── stout.rs              # The contract layer for the `stout` CLI
│   └── utils.rs              # Config, cached log handles, notifications, signal handlers
└── commands/
    └── provision.rs          # One-time provisioning from packages.toml

config/com.USER.mac-setup.plist.template   # launchd agent template
packages.toml                              # The package manifest — edit this
```

`src/core/stout.rs` deserves special attention: every assumption about how `stout` behaves
lives there, and the parsers are pure functions so they can be tested against real stout
output. If you change how we talk to `stout`, change it there.

---

## Before you open a pull request

1. **Open an issue first** for anything beyond a small fix. It is much cheaper to agree on
   an approach before you have written the code.
2. **Keep the diff focused.** Unrelated formatting changes, dependency bumps, and drive-by
   renames make a review harder, not easier.
3. **Make the tests pass** — `cargo test` must be green, and `cargo clippy -- -D warnings`
   must be clean. CI runs both.
4. **Do not weaken a test to make it pass.** If a test encodes the wrong expectation, say so
   in the issue or PR description and let the maintainer decide.
5. **Describe the risk.** If your change touches the cleanup engine, the lock file, or the
   quarantine pass, say which real failure it could cause.

---

## Coding conventions

- `cargo fmt` formatting, no exceptions — the hook enforces it.
- `cargo clippy -- -D warnings` clean.
- No `unwrap()` or `expect()` in new code paths that run in the daemon. The daemon runs
  unattended three times a day; a panic means a silent failure until the next scheduled run.
  Use `unwrap_or_default()`, `ok()`, or an explicit error path instead.
- Prefer a small pure function plus a thin wrapper over a function that shells out. The
  pure function is the one you can test.
- Keep comments about *why*, not *what*. The `stout.rs` module doc and the
  [safety invariants](#safety-invariants) section below are the models.
- Match the surrounding style: four-space indent, `snake_case` for functions and modules,
  `PascalCase` for types.

---

## Safety invariants

These were each a real bug on a real machine. They are load-bearing, and a change that
breaks one needs a very good reason and a regression test.

- **`run_with_timeout` must drain pipes concurrently.** It polls `try_wait()` only. A child
  that writes more than the ~64 KiB pipe buffer blocks forever and is then misreported as
  a timeout. See the threaded drain in `src/core/ops.rs`.
- **`cleanup_temp_files` must not delete the lock file.** It used to remove anything in
  `/tmp` older than 24 hours with no ownership check, unlinking `/tmp/mac-setup.lock` while
  the daemon held a flock on it. The flock survives on the orphaned inode, a second
  instance creates a *new* file, locks it successfully, and mutual exclusion is silently lost.
- **`HOME` must not be `expect()`ed.** `Config::default()` runs for every subcommand, so a
  missing `HOME` used to panic the daemon before a single log line was written.
- **App caches must not be wiped while the app is running.** The cask upgrader already
  detects this; the cleanup engine must too.
- **The lock is taken first, and `stout update` runs before anything else.** Every later
  step in the update daemon depends on a fresh package index.
- **Quarantine removal runs after the cask fan-out joins, never inside it.** Cask
  upgrades mutate `/Applications`.
- **Docker is not installed.** The stack is podman-only. Do not reintroduce `docker` or
  `docker-desktop` into `packages.toml`.
- **A contending update run exits 0, not 1.** launchd must not treat a skipped run as a
  failure.

---

## Testing

Tests live in `src/core/ops.rs`, `src/core/utils.rs`, `src/core/stout.rs`, and
`src/commands/provision.rs`. They are all hermetic by design:

- Do not spawn real processes.
- Do not write to the real filesystem.
- Do not depend on the machine's `HOME`, `PATH`, or installed tools.

The reference regression test is
`run_with_timeout_handles_output_larger_than_pipe_buffer`: a child writing 256 KiB must
complete well under the timeout. On the pre-fix code it hung for the full timeout and
returned `Err("Timeout exceeded")`.

If you add behaviour, add the test with it. If you fix a bug, add the test that fails
before the fix.

---

## Commit and branch conventions

Commits follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add --dry-run to the update subcommand
fix: drain stdout concurrently when a child times out
docs: describe the stout output contract
chore: bump the pinned toolchain
```

Branch names should say what the branch does: `feat/doctor-subcommand`,
`fix/plist-placeholder-substitution`, `docs/update-configuration`.

The pre-commit hook runs automatically on commit. To run the checks manually:

```bash
lefthook run pre-commit
lefthook run pre-push
```

---

## Opening issues

- Search the existing issues first.
- Use the bug template for defects, and include the log output from
  `~/Library/Logs/mac-setup-error.log` when there is one.
- Use the feature template for proposals, and describe the workflow you are trying to
  replace.
- Labels: `good first issue`, `help wanted`, `enhancement`, `documentation`, `bug`.

If an issue is unclear, leave a comment asking for the missing detail. That is a
contribution too.

---

## Community expectations

Everyone taking part in this project — in issues, in pull requests, and in discussions —
is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report anything that
breaks it to the maintainer; reports are handled confidentially.

Two practical expectations, in the spirit of it:

- Assume the other person is competent and is trying to help. "This does not work"
  beats "this is wrong".
- Review the work, not the person. A pull request that takes a different approach from
  yours is not a rejection of yours.

---

## License

`mac-setup` is released under the [MIT License](LICENSE). By contributing you agree that
your contributions are licensed under the same terms.
