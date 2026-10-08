# Task 07 — Uninstall Homebrew

**Status:** ✅ Complete  
**Date:** 2025-07-16  

---

## Summary

Homebrew was successfully uninstalled using the official uninstall script with `NONINTERACTIVE=1`.

---

## Pre-flight Check

```
/Users/organic/.local/bin/brew → stout 0.2.2   # our Rust tooling, NOT Homebrew
/opt/homebrew/bin/brew          → existed (Homebrew binary confirmed present before uninstall)
```

---

## Uninstall Command

```bash
NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/uninstall.sh)"
```

### Script Output (key lines)

```
==> Running in non-interactive mode because `$NONINTERACTIVE` is set.
==> Removing Homebrew installation...
==> /usr/bin/sudo rm -fr /opt/homebrew/Cellar
==> /usr/bin/sudo rm -fr /opt/homebrew/.git
==> Removing empty directories...
==> Homebrew uninstalled!
```

**Exit code:** 0 ✅

---

## Post-uninstall Verification

```
$ which brew 2>/dev/null || echo 'brew: not found (expected)'
/Users/organic/.local/bin/brew          # ← stout (Rust tooling), as expected

$ ls /opt/homebrew/bin/brew 2>/dev/null || echo '/opt/homebrew/bin/brew: removed (expected)'
/opt/homebrew/bin/brew: removed (expected)   ✅

$ ~/.local/bin/brew --version
stout 0.2.2                             ✅ Rust tooling fully functional
```

---

## Residual Files in /opt/homebrew

The script noted these directories were not deleted (expected behavior — the script never removes the prefix itself):

```
/opt/homebrew/Frameworks/
/opt/homebrew/bin/        (now empty — brew binary removed)
/opt/homebrew/etc/
/opt/homebrew/images/
/opt/homebrew/include/
/opt/homebrew/lib/
/opt/homebrew/man/
/opt/homebrew/opt/
/opt/homebrew/sbin/
/opt/homebrew/share/
/opt/homebrew/var/
/opt/homebrew/.DS_Store
```

These are empty or near-empty scaffold directories. They can be removed manually with:

```bash
sudo rm -rf /opt/homebrew
```

**Do not remove yet** — a later cleanup step will handle this after confirming no tools reference `/opt/homebrew` paths.

---

## Result

| Check | Result |
|-------|--------|
| Homebrew uninstall script | ✅ Exit 0 |
| `/opt/homebrew/bin/brew` removed | ✅ |
| `which brew` → stout (Rust) | ✅ |
| stout operational | ✅ `stout 0.2.2` |
| Homebrew cask/formula data removed | ✅ (`Cellar/` deleted) |

Homebrew is removed. `brew` in PATH now exclusively refers to `stout` (the Rust-based replacement).
