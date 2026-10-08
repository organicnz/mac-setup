# Shell Config Migration — Brew Removal

**Date:** 2025-07-14  
**Files modified:** `~/.zshrc`, `~/.zprofile`  
**Backups:** `~/.zshrc.backup-pre-brew-removal`, `~/.zprofile.backup-pre-brew-removal`

---

## Diff — ~/.zshrc

### Removed

```diff
-export PATH="/opt/homebrew/bin:$PATH"
```
*(was near top, after Android SDK PATH line)*

```diff
-eval "$(/opt/homebrew/bin/brew shellenv)"
```
*(was after the block of Antigravity IDE PATH entries, before GOPATH setup)*

### Already present — no changes needed

```bash
# Rust-based tooling — mise is active via:
eval "$(/Users/organic/.local/bin/mise activate zsh)"
# (line 321, end of file)

# $HOME/.local/bin is already on PATH via Qwen Code block:
export PATH='/Users/organic/.local/bin':$PATH
```

---

## Diff — ~/.zprofile

### Removed

```diff
-eval "$(/opt/homebrew/bin/brew shellenv)"
```
*(was between Kiro CLI pre block and pipx PATH line)*

### Already present — no changes needed

```bash
# $HOME/.local/bin already on PATH twice:
export PATH="$PATH:/Users/organic/.local/bin"   # pipx line
export PATH="$HOME/.local/bin:$PATH"            # LAC env block
```

---

## Summary

| File | Lines removed | Lines added |
|------|--------------|-------------|
| `~/.zshrc` | 2 (brew shellenv + homebrew/bin PATH) | 0 (mise already active) |
| `~/.zprofile` | 1 (brew shellenv) | 0 (.local/bin already present) |

Homebrew shell integration is fully removed from login and interactive shell startup. The Rust-based `mise` tooling via `~/.local/bin` remains active.
