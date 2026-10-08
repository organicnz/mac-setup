# 06 — Homebrew Removal

**Date:** 2025-01-07  
**Status:** ✅ COMPLETE  

---

## What Was Removed

| Item | Path | Size |
|------|------|------|
| `brew` bash binary | `/opt/homebrew/bin/brew` | 9 KB |
| Homebrew Ruby library | `/opt/homebrew/Library/Homebrew` | **119 MB** |

The Cellar (`/opt/homebrew/Cellar`) and all installed binaries were **preserved** — stout uses the same `/opt/homebrew` prefix as a drop-in replacement.

---

## What Was Added

| Item | Path | Purpose |
|------|------|---------|
| `brew → stout` shim | `~/.local/bin/brew` | Compatibility shim so scripts calling `brew` still work |
| `gh` binary | `/opt/homebrew/bin/gh` | Reinstalled from GitHub releases v2.102.0 (stale symlink fixed) |
| `cmake` binary + modules | `/opt/homebrew/bin/cmake` + `/opt/homebrew/share/cmake-3.31/` | Required for CMake-based stout builds |
| `libgit2` 1.9.7 | `/opt/homebrew/Cellar/libgit2/1.9.7/` | Rebuilt via stout (was an empty ghost Cellar entry) |
| `bat` symlink | `/opt/homebrew/bin/bat` | Linked to `/opt/homebrew/Cellar/bat/0.26.1/bin/bat` |

---

## Issues Encountered (Pre-existing from stout import)

These were **not caused by brew removal** — they were pre-existing ghost entries from when stout ran `import` to absorb brew's state:

| Issue | Root Cause | Fix Applied |
|-------|-----------|-------------|
| `gh` broken symlink → missing Cellar entry | brew Cellar had been cleaned after import | Downloaded official binary from GitHub releases |
| `libgit2` empty Cellar directory | Ghost entry from stout import, build failed with missing cmake | Installed cmake, rebuilt libgit2 from source via stout |
| `cmake` missing from PATH | Empty Cellar entry, stout couldn't bootstrap itself | Downloaded pre-built cmake 3.31.7 universal binary |
| `bat` missing from PATH | No opt-link, libgit2 dep was broken | Fixed libgit2 → re-linked bat via `stout link bat` |

---

## Final Verification

All 12 critical tools confirmed working after removal:

| Tool | Version | Source |
|------|---------|--------|
| git | 2.54.0 (Apple Git) | Xcode CLT |
| stout | 0.2.2 | `~/.local/bin/stout` |
| mise | 2026.10.4 | `~/.local/bin/mise` |
| gh | 2.102.0 | `/opt/homebrew/bin/gh` |
| bat | 0.26.1 | `/opt/homebrew/bin/bat` |
| docker | 29.4.0 | OrbStack |
| terraform | 1.16.5 | mise (aqua:hashicorp/terraform) |
| node | v24.21.0 | mise |
| go | 1.27.1 | mise |
| cmake | 3.31.7 | `/opt/homebrew/bin/cmake` |
| libgit2 | 1.9.7 | stout Cellar |
| brew shim | → stout 0.2.2 | `~/.local/bin/brew` |

---

## Summary

Homebrew is removed. The `/opt/homebrew` prefix remains intact as the stout-managed package store. 119 MB of Ruby infrastructure freed. A `brew` compatibility shim at `~/.local/bin/brew` redirects any remaining `brew` calls to stout transparently.

**Next steps:**
- Monitor for other ghost Cellar entries as packages are used
- Track stout cask support at https://github.com/neul-labs/stout-index for GUI apps currently installed outside of package management
- Tap support for third-party formulae (sentry-cli, openhue-cli, etc.) pending stout tap registry
