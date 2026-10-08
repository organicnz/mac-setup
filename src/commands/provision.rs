//! One-time macOS provisioning command.
//!
//! Tool responsibilities:
//!   stout  — taps, formulae, casks  (Rust-based Homebrew-compatible client)
//!   mise   — language runtimes (node, python, go, deno, ruby, …)
//!   pip    — Python packages (after mise installs python)
//!   npm    — global JS tools (after mise installs node)
//!   go     — Go binaries (after mise installs go)
//!
//! Brew is NOT used here. If a package is missing from stout's index, it is
//! logged as a warning and skipped — not silently routed through brew.

use crate::core::utils::{log, log_error, Config};
use serde::Deserialize;
use std::path::Path;
use std::process::{Command, Stdio};

// ============================================================================
// MANIFEST TYPES
// ============================================================================

#[derive(Debug, Deserialize, Default)]
pub struct Manifest {
    pub taps: Option<TapConfig>,
    pub formulae: Option<PackageList>,
    pub casks: Option<PackageList>,
    pub mise: Option<MiseConfig>,
    pub pip: Option<PackageList>,
    pub pipx: Option<PackageList>,
    pub bun: Option<BunConfig>,
    pub go: Option<PackageList>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TapConfig {
    pub taps: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PackageList {
    pub packages: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MiseConfig {
    pub tools: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct NpmConfig {
    pub global: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct BunConfig {
    pub global: Vec<String>,
}

// ============================================================================
// PROVISION STATS
// ============================================================================

#[derive(Debug, Default)]
pub struct ProvisionStats {
    pub taps_added: usize,
    pub formulae_installed: usize,
    pub casks_installed: usize,
    pub runtimes_installed: usize,
    pub pip_installed: usize,
    pub pipx_installed: usize,
    pub bun_installed: usize,
    pub go_installed: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl std::fmt::Display for ProvisionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n=== Provision Summary ===")?;
        writeln!(f, "Taps added:         {}", self.taps_added)?;
        writeln!(f, "Formulae installed: {}", self.formulae_installed)?;
        writeln!(f, "Casks installed:    {}", self.casks_installed)?;
        writeln!(f, "Runtimes (mise):    {}", self.runtimes_installed)?;
        writeln!(f, "Pip packages:       {}", self.pip_installed)?;
        writeln!(f, "Pipx apps:          {}", self.pipx_installed)?;
        writeln!(f, "Bun globals:        {}", self.bun_installed)?;
        writeln!(f, "Go tools:           {}", self.go_installed)?;
        if self.skipped > 0 {
            writeln!(f, "Skipped (already):  {}", self.skipped)?;
        }
        if self.failed > 0 {
            writeln!(f, "Failed:             {}", self.failed)?;
        }
        writeln!(f, "=========================")?;
        Ok(())
    }
}

// ============================================================================
// ENTRY POINT
// ============================================================================

pub fn run(config: &Config) -> bool {
    log("🚀 Starting mac-setup provision...", config);

    // 1. Xcode CLT
    ensure_xcode_clt(config);

    // 2. Ensure stout is available (Rust-based package client)
    if !ensure_stout(config) {
        log_error(
            "stout is required for package installation. Aborting.",
            config,
        );
        return false;
    }

    // 3. Ensure mise is available (runtime manager)
    if !ensure_mise(config) {
        log_error("mise is required for runtime management. Aborting.", config);
        return false;
    }

    // 4. Load manifest
    let manifest_path = locate_manifest();
    let manifest = match load_manifest(&manifest_path) {
        Some(m) => m,
        None => {
            log_error(
                &format!(
                    "Could not load packages.toml from {}. Aborting.",
                    manifest_path
                ),
                config,
            );
            return false;
        }
    };

    log(
        &format!("📋 Loaded manifest from {}", manifest_path),
        config,
    );

    let mut stats = ProvisionStats::default();

    // 5. Taps (via stout)
    if let Some(tap_cfg) = &manifest.taps {
        install_taps(&tap_cfg.taps, config, &mut stats);
    }

    // 6. Formulae (via stout)
    if let Some(formulae) = &manifest.formulae {
        install_formulae(&formulae.packages, config, &mut stats);
    }

    // 7. Casks (via stout)
    if let Some(casks) = &manifest.casks {
        install_casks(&casks.packages, config, &mut stats);
    }

    // 8. Runtimes (via mise)
    if let Some(mise_cfg) = &manifest.mise {
        install_mise_tools(&mise_cfg.tools, config, &mut stats);
    }

    // 9. Pip packages (after mise has installed python)
    if let Some(pip) = &manifest.pip {
        install_pip_packages(&pip.packages, config, &mut stats);
    }

    // 10. Pipx apps (isolated Python apps)
    if let Some(pipx) = &manifest.pipx {
        install_pipx_packages(&pipx.packages, config, &mut stats);
    }

    // 11. Bun globals (after mise has installed bun)
    if let Some(bun) = &manifest.bun {
        install_bun_globals(&bun.global, config, &mut stats);
    }

    // 12. Go tools (after mise has installed go)
    if let Some(go) = &manifest.go {
        install_go_tools(&go.packages, config, &mut stats);
    }

    log(&format!("{}", stats), config);

    stats.failed == 0
}

// ============================================================================
// HELPERS
// ============================================================================

fn locate_manifest() -> String {
    let candidates = [
        "packages.toml".to_string(),
        format!(
            "{}/packages.toml",
            std::env::var("HOME").unwrap_or_default()
        ),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("packages.toml")))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    ];
    for path in &candidates {
        if !path.is_empty() && Path::new(path).exists() {
            return path.clone();
        }
    }
    "packages.toml".to_string()
}

fn load_manifest(path: &str) -> Option<Manifest> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn command_available(cmd: &str) -> bool {
    if Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return true;
    }
    // Also check ~/.local/bin (stout and mise install here)
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&format!("{}/.local/bin/{}", home, cmd)).exists()
}

pub fn stout_bin() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{}/.local/bin/stout", home);
    if Path::new(&local).exists() {
        return local;
    }
    "stout".to_string()
}

pub fn mise_bin() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{}/.local/bin/mise", home);
    if Path::new(&local).exists() {
        return local;
    }
    "mise".to_string()
}

fn stout_formula_installed(name: &str) -> bool {
    let short = name.split('/').next_back().unwrap_or(name);
    Command::new(stout_bin())
        .args(["list", "--formula", short])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn stout_cask_installed(name: &str) -> bool {
    let short = name.split('/').next_back().unwrap_or(name);
    Command::new(stout_bin())
        .args(["list", "--cask", short])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn stout_tap_installed(tap: &str) -> bool {
    let output = Command::new(stout_bin())
        .arg("tap")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .any(|l| l.trim() == tap),
        Err(_) => false,
    }
}

// ============================================================================
// XCODE CLT
// ============================================================================

fn ensure_xcode_clt(config: &Config) {
    log("🔧 Checking Xcode Command Line Tools...", config);
    let installed = Command::new("xcode-select")
        .arg("-p")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if installed {
        log("✓ Xcode CLT already installed", config);
        let _ = Command::new("sudo")
            .args(["xcodebuild", "-license", "accept"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        return;
    }

    log("  Installing Xcode Command Line Tools...", config);
    log(
        "  ⚠ A system dialog may appear — click Install to continue.",
        config,
    );
    let _ = Command::new("xcode-select").arg("--install").status();
    log("✓ Xcode CLT install initiated", config);
}

// ============================================================================
// STOUT — Rust-based package client
// ============================================================================

pub fn ensure_stout(config: &Config) -> bool {
    log("🦀 Checking stout...", config);

    if command_available("stout") {
        log("✓ stout already installed", config);
        log("  Updating stout index...", config);
        let _ = Command::new(stout_bin())
            .arg("update")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        return true;
    }

    log("  Installing stout...", config);
    let status = Command::new("/bin/bash")
        .args([
            "-c",
            "curl -fsSL https://raw.githubusercontent.com/neul-labs/stout/main/install.sh | sh",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if status {
        log("✓ stout installed", config);
        true
    } else {
        log_error("Failed to install stout", config);
        false
    }
}

// ============================================================================
// MISE — runtime version manager
// ============================================================================

pub fn ensure_mise(config: &Config) -> bool {
    log("🔧 Checking mise...", config);

    if command_available("mise") {
        log("✓ mise already installed", config);
        return true;
    }

    log("  Installing mise...", config);
    let status = Command::new("/bin/bash")
        .args(["-c", "curl -fsSL https://mise.run | sh"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !status {
        log_error("Failed to install mise", config);
        return false;
    }

    // Add mise activation to ~/.zshrc if not already present
    let home = std::env::var("HOME").unwrap_or_default();
    let zshrc = format!("{}/.zshrc", home);
    let activate_line = format!(
        "eval \"$({home}/.local/bin/mise activate zsh)\"",
        home = home
    );
    if let Ok(content) = std::fs::read_to_string(&zshrc) {
        if !content.contains("mise activate") {
            let _ = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&zshrc)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "\n{}", activate_line)
                });
        }
    }

    log("✓ mise installed", config);
    true
}

// ============================================================================
// TAPS (via stout)
// ============================================================================

fn install_taps(taps: &[String], config: &Config, stats: &mut ProvisionStats) {
    if taps.is_empty() {
        return;
    }
    log(&format!("\n🔌 Adding {} taps...", taps.len()), config);

    for tap in taps {
        if stout_tap_installed(tap) {
            log(&format!("  ✓ already tapped: {}", tap), config);
            stats.skipped += 1;
            continue;
        }
        log(&format!("  Adding tap {}...", tap), config);
        let ok = Command::new(stout_bin())
            .args(["tap", tap])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ tapped: {}", tap), config);
            stats.taps_added += 1;
        } else {
            log(
                &format!("  ⚠ failed to tap: {} (not in stout index yet)", tap),
                config,
            );
            stats.failed += 1;
        }
    }
}

// ============================================================================
// FORMULAE (via stout)
// ============================================================================

fn install_formulae(packages: &[String], config: &Config, stats: &mut ProvisionStats) {
    if packages.is_empty() {
        return;
    }
    log(
        &format!("\n📦 Installing {} formulae via stout...", packages.len()),
        config,
    );

    for pkg in packages {
        if stout_formula_installed(pkg) {
            log(&format!("  ✓ already installed: {}", pkg), config);
            stats.skipped += 1;
            continue;
        }
        log(&format!("  Installing {}...", pkg), config);
        let ok = Command::new(stout_bin())
            .args(["install", pkg.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ installed: {}", pkg), config);
            stats.formulae_installed += 1;
        } else {
            log(&format!("  ⚠ failed: {} (not in stout index yet — track at github.com/neul-labs/stout-index)", pkg), config);
            stats.failed += 1;
        }
    }
}

// ============================================================================
// CASKS (via stout)
// ============================================================================

fn install_casks(packages: &[String], config: &Config, stats: &mut ProvisionStats) {
    if packages.is_empty() {
        return;
    }
    log(
        &format!("\n🖥  Installing {} casks via stout...", packages.len()),
        config,
    );

    for pkg in packages {
        if stout_cask_installed(pkg) {
            log(&format!("  ✓ already installed: {}", pkg), config);
            stats.skipped += 1;
            continue;
        }
        log(&format!("  Installing cask {}...", pkg), config);
        let ok = Command::new(stout_bin())
            .args(["install", "--cask", pkg.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ installed: {}", pkg), config);
            stats.casks_installed += 1;
        } else {
            log(
                &format!("  ⚠ failed: {} (not in stout cask index yet)", pkg),
                config,
            );
            stats.failed += 1;
        }
    }
}

// ============================================================================
// RUNTIMES (via mise)
// ============================================================================

fn install_mise_tools(tools: &[String], config: &Config, stats: &mut ProvisionStats) {
    if tools.is_empty() {
        return;
    }
    log(
        &format!("\n🔧 Installing {} runtimes via mise...", tools.len()),
        config,
    );

    for tool in tools {
        log(&format!("  Installing {}...", tool), config);
        let ok = Command::new(mise_bin())
            .args(["use", "--global", tool.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ installed: {}", tool), config);
            stats.runtimes_installed += 1;
        } else {
            log(&format!("  ⚠ failed: {}", tool), config);
            stats.failed += 1;
        }
    }
}

// ============================================================================
// PIP
// ============================================================================

fn install_pip_packages(packages: &[String], config: &Config, stats: &mut ProvisionStats) {
    if packages.is_empty() {
        return;
    }
    log(
        &format!("\n🐍 Installing {} pip packages...", packages.len()),
        config,
    );

    let python = if command_available("python3") {
        "python3"
    } else {
        "python"
    };

    let _ = Command::new(python)
        .args(["-m", "pip", "install", "--upgrade", "pip"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    for pkg in packages {
        log(&format!("  Installing {}...", pkg), config);
        let ok = Command::new(python)
            .args(["-m", "pip", "install", pkg.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ installed: {}", pkg), config);
            stats.pip_installed += 1;
        } else {
            log(&format!("  ⚠ failed: {}", pkg), config);
            stats.failed += 1;
        }
    }
}

// ============================================================================
// BUN GLOBALS (formerly NPM) — install_bun_globals defined above
// ============================================================================

// ============================================================================
// BUN GLOBALS
// ============================================================================

fn install_bun_globals(packages: &[String], config: &Config, stats: &mut ProvisionStats) {
    if packages.is_empty() {
        return;
    }
    if !command_available("bun") {
        log(
            "  ⚠ bun not found — ensure bun is installed via mise first",
            config,
        );
        return;
    }
    log(
        &format!("\n🐰 Installing {} bun globals...", packages.len()),
        config,
    );

    for pkg in packages {
        log(&format!("  Installing {}...", pkg), config);
        let ok = Command::new("bun")
            .args(["install", "--global", pkg.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ installed: {}", pkg), config);
            stats.bun_installed += 1;
        } else {
            log(&format!("  ⚠ failed: {}", pkg), config);
            stats.failed += 1;
        }
    }
}

// ============================================================================
// PIPX APPS (isolated Python applications)
// ============================================================================

fn install_pipx_packages(packages: &[String], config: &Config, stats: &mut ProvisionStats) {
    if packages.is_empty() {
        return;
    }

    // Prefer mise-managed python3 for pipx
    let python = find_mise_python();

    // Bootstrap pipx into the mise python if not present
    let pipx_ok = Command::new(&python)
        .args(["-m", "pipx", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !pipx_ok {
        log("  Bootstrapping pipx...", config);
        let _ = Command::new(&python)
            .args(["-m", "pip", "install", "--quiet", "pipx"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    log(
        &format!("\n📦 Installing {} pipx apps...", packages.len()),
        config,
    );

    for pkg in packages {
        // Check if already installed
        let installed = Command::new(&python)
            .args(["-m", "pipx", "list", "--short"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(pkg.as_str()))
            .unwrap_or(false);

        if installed {
            log(&format!("  ✓ already installed: {}", pkg), config);
            stats.skipped += 1;
            continue;
        }

        log(&format!("  Installing {}...", pkg), config);
        let ok = Command::new(&python)
            .args(["-m", "pipx", "install", pkg.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ installed: {}", pkg), config);
            stats.pipx_installed += 1;
        } else {
            log(&format!("  ⚠ failed: {}", pkg), config);
            stats.failed += 1;
        }
    }
}

fn find_mise_python() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    // Try mise shim first
    let shim = format!("{}/.local/share/mise/shims/python3", home);
    if Path::new(&shim).exists() {
        return shim;
    }
    // Fall back to mise installs
    let installs = format!("{}/.local/share/mise/installs/python", home);
    if let Ok(mut entries) = std::fs::read_dir(&installs) {
        if let Some(Ok(e)) = entries.next() {
            let candidate = format!("{}/bin/python3", e.path().display());
            if Path::new(&candidate).exists() {
                return candidate;
            }
        }
    }
    "python3".to_string()
}

// ============================================================================
// GO TOOLS
// ============================================================================

fn install_go_tools(packages: &[String], config: &Config, stats: &mut ProvisionStats) {
    if packages.is_empty() {
        return;
    }
    if !command_available("go") {
        log(
            "  ⚠ go not found — ensure go is installed via mise first",
            config,
        );
        return;
    }
    log(
        &format!("\n🐹 Installing {} go tools...", packages.len()),
        config,
    );

    for pkg in packages {
        log(&format!("  Installing {}...", pkg), config);
        let ok = Command::new("go")
            .args(["install", "-v", pkg.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            log(&format!("  ✓ installed: {}", pkg), config);
            stats.go_installed += 1;
        } else {
            log(&format!("  ⚠ failed: {}", pkg), config);
            stats.failed += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest_toml() -> &'static str {
        r#"
[taps]
taps = ["homebrew/core", "hashicorp/tap"]

[formulae]
packages = ["git", "wget", "hashicorp/tap/terraform"]

[casks]
packages = ["docker", "slack"]

[mise]
tools = ["node@lts", "python@3.12", "bun@latest", "aqua:hashicorp/terraform@latest"]

[pip]
packages = ["python-dotenv", "shodan"]

[pipx]
packages = ["azure-cli"]

[bun]
global = ["imgproxy"]

[go]
packages = ["github.com/tomnomnom/httprobe@master"]
"#
    }

    #[test]
    fn manifest_parses_all_sections() {
        let manifest: Manifest = toml::from_str(sample_manifest_toml()).unwrap();
        assert!(manifest.taps.is_some());
        assert!(manifest.formulae.is_some());
        assert!(manifest.casks.is_some());
        assert!(manifest.mise.is_some());
        assert!(manifest.pip.is_some());
        assert!(manifest.pipx.is_some());
        assert!(manifest.bun.is_some());
        assert!(manifest.go.is_some());
    }

    #[test]
    fn manifest_taps_count() {
        let manifest: Manifest = toml::from_str(sample_manifest_toml()).unwrap();
        assert_eq!(manifest.taps.unwrap().taps.len(), 2);
    }

    #[test]
    fn manifest_formulae_count() {
        let manifest: Manifest = toml::from_str(sample_manifest_toml()).unwrap();
        assert_eq!(manifest.formulae.unwrap().packages.len(), 3);
    }

    #[test]
    fn manifest_mise_tools_count() {
        let manifest: Manifest = toml::from_str(sample_manifest_toml()).unwrap();
        assert_eq!(manifest.mise.unwrap().tools.len(), 4);
    }

    #[test]
    fn manifest_pipx_packages() {
        let manifest: Manifest = toml::from_str(sample_manifest_toml()).unwrap();
        let pipx = manifest.pipx.unwrap();
        assert_eq!(pipx.packages.len(), 1);
        assert_eq!(pipx.packages[0], "azure-cli");
    }

    #[test]
    fn manifest_bun_global() {
        let manifest: Manifest = toml::from_str(sample_manifest_toml()).unwrap();
        let bun = manifest.bun.unwrap();
        assert_eq!(bun.global[0], "imgproxy");
    }

    #[test]
    fn manifest_empty_is_valid() {
        let manifest: Manifest = toml::from_str("").unwrap();
        assert!(manifest.taps.is_none());
        assert!(manifest.formulae.is_none());
        assert!(manifest.mise.is_none());
    }

    #[test]
    fn stout_bin_returns_string() {
        let bin = stout_bin();
        assert!(!bin.is_empty());
        // Must end with "stout"
        assert!(bin.ends_with("stout"));
    }

    #[test]
    fn mise_bin_returns_string() {
        let bin = mise_bin();
        assert!(!bin.is_empty());
        assert!(bin.ends_with("mise"));
    }

    #[test]
    fn provision_stats_display_contains_sections() {
        let stats = ProvisionStats {
            taps_added: 2,
            formulae_installed: 10,
            casks_installed: 5,
            runtimes_installed: 4,
            pip_installed: 3,
            pipx_installed: 1,
            bun_installed: 1,
            go_installed: 1,
            skipped: 20,
            failed: 0,
        };
        let out = format!("{}", stats);
        assert!(out.contains("Taps added:"));
        assert!(out.contains("Formulae installed:"));
        assert!(out.contains("Runtimes (mise):"));
        assert!(out.contains("Pipx apps:"));
        assert!(out.contains("Skipped (already):"));
    }

    #[test]
    fn provision_stats_failed_shown_only_when_nonzero() {
        let mut stats = ProvisionStats::default();
        let out_no_fail = format!("{}", stats);
        assert!(!out_no_fail.contains("Failed:"));

        stats.failed = 3;
        let out_with_fail = format!("{}", stats);
        assert!(out_with_fail.contains("Failed:"));
    }
}
