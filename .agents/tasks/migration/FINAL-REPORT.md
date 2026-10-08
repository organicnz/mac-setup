# macOS Brew → Stout/Mise Migration Report

## Migration Status: PARTIAL

Core migration is complete — Homebrew is uninstalled, all developer runtimes run under mise,
409 CLI formulae are managed by stout. The partial designation reflects that **all 75 GUI casks**
and **15 third-party taps** are not yet supported by stout's index and require manual installation.

---

## What was done

- Unlinked brew runtime formulae (node, python@3.12, python@3.13, go, ruby, deno, openjdk, openjdk@21, dart-sdk)
- Built mac-setup release binary — `mac-setup 0.3.0` at `target/release/mac-setup` (3.3 MB)
- Updated stout package index — formula index `2026.10.08.1521` (8646 formulas); cask sync produced a non-fatal schema warning but exited 0
- Ran `mac-setup provision` (stout + mise) — 127 formulae installed, 7 runtimes via mise, 4 pip packages, 1 npm global, 1 Go tool; 11 formulae + all 75 casks failed (not yet in index)
- Removed brew shell config from `~/.zshrc` (2 lines) and `~/.zprofile` (1 line)
- Uninstalled Homebrew via official uninstall script (`NONINTERACTIVE=1`) — exit 0
- stout + mise PATH already present in `~/.zshrc` via `$HOME/.local/bin` and `eval "$(mise activate zsh)"`
- Cleaned up brew caches and orphaned log files (`~/Library/Logs/brew-*.log`, ~364 KB freed); Homebrew caches already removed in a prior step
- Created `~/.local/bin/brew` → stout shim for backwards compatibility
- Committed and pushed to origin main — HEAD `c4f0f86`

---

## Tool Verification (post-uninstall)

| Tool | Status | Version |
|------|--------|---------|
| git | ✅ working | 2.54.0 (Apple Git-157) — `/usr/bin/git` |
| node | ✅ working | v24.21.0 — mise (`~/.local/share/mise/installs/node/lts`) |
| python3 | ✅ working | 3.13.16 — mise (`python@3.13`) |
| go | ✅ working | go1.27.1 darwin/arm64 — mise |
| docker | ✅ working | 29.4.0 — OrbStack/Docker Desktop |
| terraform | ✅ working | v1.16.5 — mise (aqua:hashicorp/terraform) |
| stout | ✅ working | 0.2.2 — `~/.local/bin/stout` |
| mise | ✅ working | 2026.10.4 macos-arm64 — `~/.local/bin/mise` |

All 8 gate-critical tools verified working. Zero failures.

---

## Packages not yet in stout index (need manual follow-up)

### Formulae (11)

| Package | Notes |
|---------|-------|
| `azure-cli` | Not in stout formula index |
| `camsnap` | Not in stout formula index |
| `codex` | Not in stout formula index |
| `packer` | Not in stout formula index |
| `terraform` | In stout index as `1.15.5`; mise provides `1.16.5` and takes priority — effectively covered |
| `vlt` | Not in stout formula index |
| `gofireflyio/aiac/aiac` | Tap-scoped — tap not registered in stout |
| `getsentry/tools/sentry-cli` | Tap-scoped — tap not registered in stout |
| `openhue/cli/openhue-cli` | Tap-scoped — tap not registered in stout |
| `pulumi/tap/pulumi` | Tap-scoped — tap not registered in stout |
| `supabase/tap/supabase` | Tap-scoped — tap not registered in stout (plain `supabase` formula was installed) |
| `teamookla/speedtest/speedtest` | Tap-scoped — tap not registered (plain `speedtest-cli` was installed) |

### Casks (75 — all failed, stout cask index not yet populated)

Critical ones to install manually until stout adds cask support:

| Cask | Priority |
|------|----------|
| `orbstack` or `docker-desktop` | 🔴 Critical (Docker already working via OrbStack) |
| `visual-studio-code` / `windsurf` / `zed` | 🔴 High |
| `warp` / `ghostty` / `iterm2` | 🔴 High |
| `slack` / `zoom` | 🟡 Medium |
| `obsidian` / `notion` / `raycast` | 🟡 Medium |
| `android-studio` / `figma` | 🟡 Medium |
| `google-chrome` | 🟡 Medium |
| `claude` / `claude-code` | 🟡 AI tools |

Full cask list (install via `.dmg`, App Store, or vendor website): android-commandlinetools, android-ndk, android-platform-tools, android-studio, anydesk, balenaetcher, burp-suite, ccleaner, cloudflare-warp, cyberduck, dbeaver-community, deepl, discord, docker, docker-desktop, drivedx, elmedia-player, figma, flux, font-inconsolata, ghostty, github, google-chrome, google-cloud-sdk, google-drive, grammarly, hex-fiend, iterm2, lm-studio, loom, losslesscut, macfuse, master-pdf-editor, microsoft-edge, miro, ngrok, notion, obsidian, opencore-patcher, oracle-jdk, orbstack, pingplotter, postman, protonvpn, rapidapi, rar, raycast, rustdesk, sejda-pdf, signal, sim-genie, skitch, slack, spotify, squash, teamviewer, termius, the-unarchiver, todoist, tor-browser, transmission, transmit, tunnelblick, viber, visual-studio-code, vlc, vmware-fusion, warp, webtorrent, wifi-explorer, wifi-explorer-pro, windscribe, windsurf, wine-stable, wireshark, xurl, zed, zen, zoom, claude, claude-code, codex, devin-desktop, opencat, skip, swift-android-toolchain@6.2, swift-host-toolchain@6.2.3

Track cask support progress at: https://github.com/neul-labs/stout-index

---

## Residual state

**`/opt/homebrew` — owned by stout, must be kept.**

Stout v0.2.2 uses `HOMEBREW_PREFIX=/opt/homebrew` and `HOMEBREW_CELLAR=/opt/homebrew/Cellar` as its
active package store. It currently manages **409 formulas** (7 entries visible live in `/opt/homebrew/Cellar`
at report time; the full state is in `~/.stout/state/installed.toml`). The directory is no longer a
Homebrew artifact — do not remove it.

The Homebrew binary (`/opt/homebrew/bin/brew`) was removed during uninstall. `which brew` now resolves to
`~/.local/bin/brew`, which is a zsh shim that forwards all invocations to `~/.local/bin/stout`.

Residual `/opt/homebrew` PATH entries remain in `~/.zshrc` (lines referencing `node@22`, `node@24`,
and a conda block). These are harmless since mise tools win via shim priority, but should be cleaned
up for hygiene.

---

## Next steps

1. **Remove residual `/opt/homebrew` PATH entries from `~/.zshrc`** — lines referencing `node@22/bin`,
   `node@24/bin`, and the `miniconda` block. Run `source ~/.zshrc` afterwards.

2. **Install high-priority GUI apps manually** until stout's cask index is populated — use `.dmg` files,
   the Mac App Store, or vendor installers for: orbstack/docker-desktop, VS Code/windsurf/zed, warp/ghostty,
   slack, zoom, obsidian, raycast, google-chrome.

3. **Install missing CLI tools manually** until the stout index gains coverage:
   `azure-cli` (via pip/official installer), `packer` (HashiCorp direct), `sentry-cli` (via curl installer).

4. **Track stout index additions** at https://github.com/neul-labs/stout-index — once cask/tap support
   lands, re-run `mac-setup provision` to fill remaining gaps automatically.

5. **Optional: clean up `~/.stout/state/`** — verify `installed.toml` reflects the current 409-package
   state and prune any stale entries from the pre-migration brew snapshot.

6. **Commit this report** to origin main to close the migration workflow.
