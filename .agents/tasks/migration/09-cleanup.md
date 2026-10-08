# Step 09 — Post-Migration Cleanup Report

**Date:** 2025-10-08  
**Status:** ✅ Complete

---

## Stout Prefix

Stout v0.2.2 is configured with:

| Key | Value |
|-----|-------|
| `HOMEBREW_PREFIX` | `/opt/homebrew` |
| `HOMEBREW_CELLAR` | `/opt/homebrew/Cellar` |
| `STOUT_DIR` | `~/.stout` |
| `STOUT_CACHE` | `~/.stout/downloads` |
| `CONFIG_FILE` | `~/.stout/config.toml` |
| `INSTALLED_FILE` | `~/.stout/state/installed.toml` |

Stout reuses the same `/opt/homebrew` Cellar layout that Homebrew used. It currently manages **409 formulas**.

---

## What Was Safely Removed

| Artifact | Action | Reason |
|----------|--------|--------|
| `~/Library/Logs/brew-update-stderr.log` | ✅ Deleted | Residual log from old brew-auto-update cron/launchd job |
| `~/Library/Logs/brew-update-stdout.log` | ✅ Deleted | Same |
| `~/Library/Logs/brew-updates-error.log` | ✅ Deleted | Same |
| `~/Library/Logs/brew-updates.log` | ✅ Deleted | Same |

Total reclaimed: ~364 KB of orphaned brew log files.

---

## What Was Already Gone

| Artifact | Status |
|----------|--------|
| `~/Library/Caches/Homebrew` | Already removed in a prior step |
| `~/Library/Logs/Homebrew/` | Already removed in a prior step |
| `brew` binary in `/opt/homebrew/bin` | Not present (Homebrew uninstalled cleanly) |
| `brew` binary in `/usr/local/bin` | Not present |

---

## What Was Kept (and Why)

| Artifact | Kept Because |
|----------|-------------|
| `/opt/homebrew/` (entire tree) | **Stout owns this directory.** `HOMEBREW_PREFIX=/opt/homebrew` and `HOMEBREW_CELLAR=/opt/homebrew/Cellar`. Deleting it would wipe all 409 stout-managed packages. Do not remove. |
| `~/.local/bin/brew` | This is the **brew → stout shim** created during migration. It redirects any `brew` invocations to `~/.local/bin/stout` for compatibility with scripts that still call `brew`. Keep it. |

---

## Decision: /opt/homebrew

**Leave it. Stout owns it.**

Stout is already installed and managing packages from `/opt/homebrew/Cellar`. The directory is no longer a Homebrew artifact — it is stout's active package store. Removing it would uninstall all 409 managed packages.

---

## Summary

The migration is clean. Homebrew is fully replaced:
- The `brew` binary is gone from all standard paths.
- The `~/.local/bin/brew` shim redirects to stout for backwards compatibility.
- Stout manages 409 formulas from `/opt/homebrew/Cellar`.
- All brew-specific caches and logs have been removed.
- No orphaned content was found at `/opt/homebrew` — stout owns it entirely.
