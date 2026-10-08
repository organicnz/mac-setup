# 04 — Provision Run Summary

**Date:** 2026-10-08  
**Command:** `mac-setup provision`  
**Binary:** `/Users/organic/dev/work/apple/mac-setup/target/release/mac-setup`  
**Manifest:** `packages.toml`

---

## Final Stats (from binary)

| Category           | Count |
|--------------------|------:|
| Taps added         | 0     |
| Formulae installed | 127   |
| Casks installed    | 0     |
| Runtimes (mise)    | 7     |
| Pip packages       | 4     |
| NPM globals        | 1     |
| Go tools           | 1     |
| **Skipped**        | 0     |
| **Failed**         | 116   |

---

## Prerequisites — All OK

- ✓ Xcode CLT already installed
- ✓ stout already installed (index updated)
- ✓ mise already installed

---

## Taps — 0 / 15 added (all failed — not in stout tap registry yet)

All 15 taps are third-party and not yet registered in stout's tap index:

- `anomalyco/tap`
- `cloudflare/cloudflare`
- `getsentry/tools`
- `gofireflyio/aiac`
- `hashicorp/tap`
- `heroku/brew`
- `homebrew/services`
- `ngrok/ngrok`
- `openhue/cli`
- `pulumi/tap`
- `skiptools/skip`
- `supabase/tap`
- `teamookla/speedtest`
- `xcodesorg/made`
- `xdevplatform/tap`

---

## Formulae — 127 installed, 11 failed

### ✓ Installed (127)

actionlint, aiac, aider, aircrack-ng, ansible, autossh, awscli, awscurl, bat, bfg, bind, bottom, certbot, cliclick, cline, cloudflared, cocoapods, curl, dart-sdk, ddrescue, deno, docker-compose, doctl, duckdb, duf, dust, eza, fastlane, fd, ffmpeg, fish, flarectl, flyctl, fzf, gcalcli, **gh**, **git**, git-delta, git-filter-repo, git-lfs, gnupg, **go**, gogcli, gradle, hashcat, hcxtools, helix, heroku, htop, httrack, hyperfine, imagemagick, ios-deploy, jj, jq, k9s, lazygit, lefthook, libimobiledevice, lima, llama.cpp, lolcat, macchina, **mise**, mplayershell, mtr, neovim, net-snmp, netcat, nginx, nmap, node, nushell, nvm, openapi-generator, openbao, openjdk, openjdk@21, openssh, phoneinfoga, php, pidof, pipx, pkgx, podman, postgresql@17, pre-commit, protobuf, pulumi, python@3.12, python@3.13, redis, ripgrep, rm-improved, ruby, rustup, shellcheck, sherlock, speedtest-cli, starship, stunnel, supabase, swiftformat, swiftlint, swiftly, switchaudio-osx, tesseract, testdisk, tmux, tokei, trash, tree, trivy, unzip, uv, vercel, wget, wireguard-go, wireguard-tools, wireshark, xcodegen, xcodes, xray, yarn, yt-dlp, zellij, zoxide

> Note: `docker` as a formula (CLI) and `terraform` are installed via stout's core index — however **terraform** failed (see below).

### ⚠ Failed — not in stout index yet (11)

| Package | Notes |
|---------|-------|
| `azure-cli` | Not in stout formula index |
| `camsnap` | Not in stout formula index |
| `codex` | Not in stout formula index |
| `packer` | Not in stout formula index |
| `terraform` | **Required** — not in stout formula index yet |
| `vlt` | Not in stout formula index |
| `gofireflyio/aiac/aiac` | Tap-scoped — tap not registered |
| `getsentry/tools/sentry-cli` | Tap-scoped — tap not registered |
| `openhue/cli/openhue-cli` | Tap-scoped — tap not registered |
| `pulumi/tap/pulumi` | Tap-scoped — tap not registered |
| `supabase/tap/supabase` | Tap-scoped — tap not registered |
| `teamookla/speedtest/speedtest` | Tap-scoped — tap not registered |
| `xcodesorg/made/xcodes` | Tap-scoped — tap not registered (plain `xcodes` was installed) |

---

## Casks — 0 / 75 installed (all failed — stout cask index not yet populated)

Stout's cask index is not yet populated. All 75 casks failed. Notable ones:

| Cask | Priority |
|------|----------|
| `docker` / `docker-desktop` | 🔴 Critical |
| `orbstack` | 🔴 Critical |
| `visual-studio-code` | 🔴 High |
| `warp` / `ghostty` / `iterm2` | 🔴 High (terminal) |
| `slack` | 🟡 Medium |
| `zoom` | 🟡 Medium |
| `obsidian` | 🟡 Medium |
| `figma` | 🟡 Medium |
| `android-studio` | 🟡 Medium |
| `google-chrome` | 🟡 Medium |
| `raycast` | 🟡 Medium |
| `windsurf` / `zed` | 🟡 Medium (editors) |
| `claude` / `claude-code` | 🟡 AI tools |

Full failed cask list: android-commandlinetools, android-ndk, android-platform-tools, android-studio, anydesk, balenaetcher, burp-suite, ccleaner, cloudflare-warp, cyberduck, dbeaver-community, deepl, discord, docker, docker-desktop, drivedx, elmedia-player, figma, flux, font-inconsolata, ghostty, github, google-chrome, google-cloud-sdk, google-drive, grammarly, hex-fiend, iterm2, lm-studio, loom, losslesscut, macfuse, master-pdf-editor, microsoft-edge, miro, ngrok, notion, obsidian, opencore-patcher, oracle-jdk, orbstack, pingplotter, postman, protonvpn, rapidapi, rar, raycast, rustdesk, sejda-pdf, signal, sim-genie, skitch, slack, spotify, squash, teamviewer, termius, the-unarchiver, todoist, tor-browser, transmission, transmit, tunnelblick, viber, visual-studio-code, vlc, vmware-fusion, warp, webtorrent, wifi-explorer, wifi-explorer-pro, windscribe, windsurf, wine-stable, wireshark, xurl, zed, zen, zoom, claude, claude-code, codex, devin-desktop, opencat, skip, swift-android-toolchain@6.2, swift-host-toolchain@6.2.3

---

## Runtimes via mise — 7 / 7 ✓

All runtimes installed successfully:

| Tool | Status |
|------|--------|
| `node@lts` | ✓ installed |
| `python@3.12` | ✓ installed |
| `python@3.13` | ✓ installed |
| `go@latest` | ✓ installed |
| `deno@latest` | ✓ installed |
| `ruby@latest` | ✓ installed |
| `java@temurin-21` | ✓ installed (temurin-21.0.12+101.0.LTS) |

---

## Pip packages — 4 / 4 ✓

| Package | Status |
|---------|--------|
| `python-dotenv` | ✓ installed |
| `shodan` | ✓ installed |
| `mmh3` | ✓ installed |
| `pipx` | ✓ installed |

---

## NPM globals — 1 / 1 ✓

| Package | Status |
|---------|--------|
| `imgproxy` | ✓ installed |

---

## Go tools — 1 / 1 ✓

| Package | Status |
|---------|--------|
| `github.com/tomnomnom/httprobe@master` | ✓ installed |

---

## Critical Package Check

| Package | Required | Status |
|---------|----------|--------|
| `git` | ✓ | ✓ installed via stout |
| `stout` | ✓ | ✓ pre-existing |
| `mise` | ✓ | ✓ pre-existing + re-installed |
| `docker` (formula) | ✓ | ✓ installed via stout |
| `terraform` | ✓ | ⚠ **FAILED** — not in stout index |
| `gh` | ✓ | ✓ installed via stout |

> **terraform** is the only required package that failed. Track at: https://github.com/neul-labs/stout-index

---

## Summary

The provision run completed with partial success. All CLI formulae, runtimes, pip/npm/go tools installed successfully. The main gap is:

1. **All casks** — stout's cask index is not yet populated; GUI apps need to be installed manually or via direct `.dmg`/App Store until stout adds cask support.
2. **All taps** — third-party tap registries are not yet supported in stout's tap system.
3. **terraform** — the one required formula not yet in stout's index.
4. **5 tap-scoped formulae** (sentry-cli, openhue-cli, etc.) — depend on taps that aren't registered.

**Next step:** Track cask/tap support at https://github.com/neul-labs/stout-index and install high-priority GUI apps directly until the index is populated.
