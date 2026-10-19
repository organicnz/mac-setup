use clap::{Parser, Subcommand};
use mac_setup::commands::provision;
use mac_setup::core::cleanup::DRY_RUN;
use mac_setup::core::utils::{
    install_signal_handlers, log, log_error, send_notification, shutdown_requested, stout_cmd,
    Config,
};
use mac_setup::{checks, cleanup, ops, stats};
use std::path::Path;

// ============================================================================
// CLI DEFINITION
// ============================================================================

#[derive(Parser)]
#[command(
    name = "mac-setup",
    version,
    about = "Unified macOS package management — provision, update, and maintain your system",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// One-time provisioning: install Xcode CLT, stout, mise, and all packages from packages.toml
    Provision,

    /// Run the automated update daemon (stout update + upgrade + bun + cleanup)
    Update,

    /// Remove broken casks, fix bun issues, and run housekeeping
    Fix,

    /// Run pre-commit audit checks (plist, secrets, markdown)
    Audit {
        /// Type of check to run: plist | secrets | markdown
        check: String,
        /// Files to check
        files: Vec<String>,
    },

    /// Set up the development environment (lefthook, clippy, rustfmt)
    Setup,

    /// Run comprehensive system cleanup standalone
    Cleanup {
        /// Run aggressive cleanup (removes more cached data)
        #[arg(long)]
        aggressive: bool,
        /// Dry run: report what would be freed without deleting anything
        #[arg(long)]
        dry_run: bool,
    },

    /// Install or reinstall the launchd agent for scheduled updates
    Install,
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    let cli = Cli::parse();
    let config = Config::default();

    // Catch SIGTERM/SIGINT so a `launchctl bootout` or reboot lets the daemon finish
    // the cask it is on rather than dying mid-install.
    install_signal_handlers();

    match cli.command {
        Commands::Provision => run_provision(&config),
        Commands::Update => run_update(&config),
        Commands::Fix => run_fix(&config),
        Commands::Audit { check, files } => run_audit(&check, &files),
        Commands::Setup => run_setup(&config),
        Commands::Cleanup {
            aggressive,
            dry_run,
        } => run_cleanup(&config, aggressive, dry_run),
        Commands::Install => run_install(&config),
    }
}

// ============================================================================
// PROVISION
// ============================================================================

fn run_provision(config: &Config) {
    log("🚀 mac-setup provision", config);
    let success = provision::run(config);
    if success {
        send_notification("Success", "Provisioning complete!");
        std::process::exit(0);
    } else {
        send_notification("Warning", "Provisioning completed with errors");
        std::process::exit(1);
    }
}

// ============================================================================
// UPDATE (former main.rs daemon logic)
// ============================================================================

fn run_update(config: &Config) {
    use fs2::FileExt;
    use std::fs::File;

    log("🚀 Starting mac-setup update", config);
    log(
        &format!("   Version: {}", env!("CARGO_PKG_VERSION")),
        config,
    );

    // Acquire lock
    let lock_file = match File::create(&config.lock_file) {
        Ok(f) => f,
        Err(e) => {
            log_error(&format!("Failed to create lock file: {}", e), config);
            std::process::exit(1);
        }
    };

    match lock_file.try_lock_exclusive() {
        Ok(_) => log("🔒 Lock acquired", config),
        Err(_) => {
            log("⚠ Another instance is running, exiting...", config);
            std::process::exit(0);
        }
    }

    // Pre-flight checks
    log("\n📡 Running pre-flight checks...", config);

    if !checks::check_network(config) {
        log_error("Aborting: No network connectivity", config);
        send_notification("Skipped", "No network connection available");
        std::process::exit(1);
    }

    // Pre-task cleanup
    log("\n", config);
    let pre_cleanup_stats = cleanup::comprehensive_cleanup(config, false);

    // Disk space check
    if !checks::check_disk_space(config) {
        log("⚠ Low disk space, running aggressive cleanup...", config);
        let aggressive_stats = cleanup::comprehensive_cleanup(config, true);

        if !checks::check_disk_space(config) {
            log_error(
                "Aborting: Insufficient disk space even after cleanup",
                config,
            );
            send_notification("Skipped", "Insufficient disk space");
            std::process::exit(1);
        }

        log(
            &format!(
                "✓ Aggressive cleanup freed additional {}MB",
                aggressive_stats.total_mb_freed()
            ),
            config,
        );
    }

    if !ops::check_stout(config) {
        log_error("Aborting: stout not found", config);
        send_notification("Skipped", "stout not found");
        std::process::exit(1);
    }

    let mut update_stats = stats::UpdateStats::default();
    let mut overall_success = true;

    // Update operations
    log("\n📦 Running package updates...", config);

    // `stout update` refreshes the index every other step depends on, so it runs
    // first and on its own.
    if !ops::update_stout(config) {
        overall_success = false;
    }

    // Everything below is split across independent prefixes. stout formulae+casks share
    // a single prefix and stout's own locks, so they stay in a single sequential
    // thread; mise, pipx and bun are genuinely independent and run alongside.
    //
    // Previously all of these ran strictly back to back, so wall-clock cost was the
    // sum of four unrelated network-bound phases.
    //
    // The other load-bearing ordering constraint: cask upgrades mutate /Applications,
    // so the quarantine passes must not overlap them. Those run after the fan-out
    // joins, not inside it.
    log(
        "  ↗ Running stout, mise, pipx and bun upgrades in parallel...",
        config,
    );

    let (stout_result, mise_ok, pipx_ok, bun) = std::thread::scope(|scope| {
        let stout_handle = scope.spawn(|| {
            let formulae = ops::upgrade_formulae(config);
            let casks = ops::upgrade_casks(config);
            (formulae, casks)
        });

        // These report success/failure or return their own stats, so they never
        // touch the shared UpdateStats.
        let mise_handle = scope.spawn(|| ops::upgrade_mise(config));
        let pipx_handle = scope.spawn(|| ops::upgrade_pipx(config));
        let bun_handle = scope.spawn(|| ops::update_bun(config));

        // A panicking worker must not take the daemon down with it:
        // treat it as a failed upgrade and keep going so the rest of
        // the run still reports.
        let stout_result = match stout_handle.join() {
            Ok(result) => (result, false),
            Err(_) => (
                (
                    stats::ComponentStats::default(),
                    stats::CaskStats::default(),
                ),
                true,
            ),
        };
        let mise_ok = mise_handle.join().unwrap_or(false);
        let pipx_ok = pipx_handle.join().unwrap_or(false);
        let bun = bun_handle.join().unwrap_or_else(|_| Default::default());

        (stout_result, mise_ok, pipx_ok, bun)
    });

    let ((formulae, casks), stout_panicked) = stout_result;

    // Hard failures: the update infrastructure itself failed, a worker
    // panicked, or stout's output could not be parsed (drift). Drift is
    // a hard failure because we cannot confirm the run did anything.
    if !mise_ok || !pipx_ok || stout_panicked {
        overall_success = false;
    }
    if !formulae.errors.is_empty() || !bun.errors.is_empty() {
        overall_success = false;
    }

    update_stats.formulae = formulae;
    update_stats.casks = casks;
    update_stats.bun = bun;

    // Package-level skips and warnings are needs-attention, not failures:
    // a cask blocked on an auth prompt or a formula with no bottle needs a
    // human, but the daemon did its job. They surface as a soft "some
    // items require attention" (exit 0) rather than a green "all success".
    let needs_attention = update_stats.casks.has_actionable_items()
        || update_stats.formulae.has_problems()
        || update_stats.bun.has_problems();

    // Quarantine stripping runs after the cask upgrades have settled, since it reads
    // the freshly installed .app bundles.
    ops::remove_all_quarantine(config);
    ops::remove_all_formula_quarantine(config);

    // If we were signalled partway through, stop before the second cleanup sweep
    // rather than starting a long operation we cannot finish.
    if shutdown_requested() {
        log(
            "\n⚠ Shutdown requested — skipping post-cleanup and finishing early",
            config,
        );
        send_notification(
            "Stopped",
            "Shutdown requested; run again to finish remaining updates",
        );
        std::process::exit(1);
    }

    // Post-task cleanup. The pre-flight sweep above already ran every stage; running
    // the identical sweep again immediately afterwards doubled the cost of every run
    // for no benefit. Only the post-upgrade residue is swept here.
    log("\n", config);
    let post_cleanup_stats = cleanup::cleanup_after_upgrades(config);

    // Summary
    log("\n", config);
    log(
        "═══════════════════════════════════════════════════════════",
        config,
    );
    log(
        "                    UPDATE SUMMARY                         ",
        config,
    );
    log(
        "═══════════════════════════════════════════════════════════",
        config,
    );

    log(&format!("{}", update_stats), config);

    let total_freed_mb = pre_cleanup_stats.total_mb_freed() + post_cleanup_stats.total_mb_freed();
    let total_files = pre_cleanup_stats.files_removed + post_cleanup_stats.files_removed;

    log("\n📊 Cleanup Summary:", config);
    log(
        &format!("   Total space freed: {}MB", total_freed_mb),
        config,
    );
    log(&format!("   Files/dirs removed: {}", total_files), config);

    macro_rules! log_freed {
        ($label:expr, $pre:expr, $post:expr) => {
            let total = $pre + $post;
            if total > 0 {
                log(&format!("   {}: {}MB", $label, total / 1_000_000), config);
            }
        };
    }

    log_freed!(
        "stout",
        pre_cleanup_stats.stout_cache_freed,
        post_cleanup_stats.stout_cache_freed
    );
    log_freed!(
        "Bun",
        pre_cleanup_stats.bun_cache_freed,
        post_cleanup_stats.bun_cache_freed
    );
    log_freed!(
        "Cargo",
        pre_cleanup_stats.cargo_cache_freed,
        post_cleanup_stats.cargo_cache_freed
    );
    log_freed!(
        "System caches",
        pre_cleanup_stats.system_cache_freed,
        post_cleanup_stats.system_cache_freed
    );

    let total_logs = pre_cleanup_stats.logs_cleaned + post_cleanup_stats.logs_cleaned;
    if total_logs > 0 {
        log(&format!("   Logs rotated: {}", total_logs), config);
    }

    // Disk space
    log_disk_space(config);

    log(
        "\n═══════════════════════════════════════════════════════════",
        config,
    );

    if overall_success {
        if needs_attention {
            mac_setup::core::utils::log(
                "✅ Updates completed with some items requiring attention",
                config,
            );
            send_notification("Success", "Updates completed - some items need attention");
            std::process::exit(0);
        } else {
            mac_setup::core::utils::log("✅ All updates completed successfully!", config);
            send_notification("Success", &format!("Updated! Freed {}MB", total_freed_mb));
            std::process::exit(0);
        }
    } else {
        mac_setup::core::utils::log("⚠️ Updates completed with some errors", config);
        send_notification("Warning", "Updates completed with some errors");
        std::process::exit(1);
    }
}

// ============================================================================
// FIX
// ============================================================================

fn run_fix(config: &Config) {
    use std::process::{Command, Stdio};

    log("==========================================", config);
    log("      mac-setup fix                       ", config);
    log("==========================================", config);

    let casks_to_remove = [
        "airtable",
        "amazon-chime",
        "asana",
        "bitwarden",
        "blender",
        "brave-browser",
        "canva",
        "chatgpt",
        "cleanmymac",
        "clickup",
        "evernote",
        "firefox",
        "freecad",
        "geekbench",
        "google-chrome",
        "imazing",
        "losslesscut",
        "pgadmin4",
        "wondershare-uniconverter",
        "rustdesk",
        "xmind",
        "speedify",
        "youtube-to-mp3",
        "outline-manager",
        "ubiquiti-unifi-controller",
    ];

    let mut failures = 0usize;

    log("\n[1/4] Cleaning up broken/missing casks...", config);
    for cask in casks_to_remove {
        let status = Command::new(stout_cmd())
            .args(["list", "--cask", cask])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if let Ok(s) = status {
            if s.success() {
                let uninstall_status = Command::new(stout_cmd())
                    .args(["uninstall", "--cask", cask])
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .status();

                match uninstall_status {
                    Ok(us) if us.success() => log(&format!("  - ✓ Uninstalled {}", cask), config),
                    _ => {
                        failures += 1;
                        log_error(&format!("  - ✗ Failed to uninstall {}", cask), config);
                    }
                }
            } else {
                log(&format!("  - {} already removed.", cask), config);
            }
        }
    }

    log("\n[2/4] Fixing Flutter (Invalid definition)...", config);
    let flutter_check = Command::new(stout_cmd())
        .args(["list", "--cask", "flutter"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if let Ok(s) = flutter_check {
        if s.success() {
            let status = Command::new(stout_cmd())
                .args(["uninstall", "--cask", "flutter"])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .status();
            match status {
                Ok(us) if us.success() => log("  - ✓ Uninstalled flutter", config),
                _ => {
                    failures += 1;
                    log_error("  - ✗ Failed to uninstall flutter", config);
                }
            }
        } else {
            log("  - Flutter already removed.", config);
        }
    }

    log("\n[3/4] Fixing invalid bun global package...", config);
    let bun_status = Command::new("bun")
        .args(["remove", "--global", "@anthropic-ai/.claude-code-2DTsDk1V"])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status();
    match bun_status {
        Ok(s) if s.success() => log("  - ✓ Removed invalid package", config),
        _ => {
            failures += 1;
            log("  - ✗ Failed or package not present", config);
        }
    }

    let cache_status = Command::new("bun")
        .args(["pm", "cache", "rm"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match cache_status {
        Ok(s) if s.success() => log("  - ✓ Cleaned bun cache", config),
        _ => {
            failures += 1;
            log_error("  - ✗ Failed to clean bun cache", config);
        }
    }

    log("\n[4/4] Final Housekeeping...", config);

    let cleanup = Command::new(stout_cmd())
        .args(["cleanup", "-s"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if cleanup.map(|s| s.success()).unwrap_or(false) {
        log("  - ✓ stout cleanup complete", config);
    } else {
        failures += 1;
        log_error("  - ✗ stout cleanup failed", config);
    }

    let autoremove = Command::new(stout_cmd())
        .arg("autoremove")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if autoremove.map(|s| s.success()).unwrap_or(false) {
        log("  - ✓ stout autoremove complete", config);
    } else {
        failures += 1;
        log_error("  - ✗ stout autoremove failed", config);
    }

    log("  - Checking stout doctor...", config);
    let _ = Command::new(stout_cmd()).arg("doctor").status();

    log("\n==========================================", config);
    if failures == 0 {
        log("Fix Complete!", config);
    } else {
        log_error(
            &format!("Fix finished with {failures} failed step(s)"),
            config,
        );
    }

    log_disk_space(config);
    log("==========================================", config);

    if failures > 0 {
        std::process::exit(1);
    }
}

// ============================================================================
// AUDIT
// ============================================================================

fn run_audit(check: &str, files: &[String]) {
    if files.is_empty() {
        eprintln!("Usage: mac-setup audit <check> <files...>");
        std::process::exit(1);
    }

    let mut has_error = false;

    match check {
        "plist" => {
            println!("🔍 Validating plist templates...");
            for file in files {
                if !check_plist(file) {
                    has_error = true;
                }
            }
        }
        "secrets" => {
            println!("🔍 Checking for secrets...");
            for file in files {
                if !check_secrets(file) {
                    has_error = true;
                }
            }
        }
        "markdown" => {
            println!("🔍 Checking markdown files...");
            for file in files {
                if !check_markdown(file) {
                    has_error = true;
                }
            }
        }
        _ => {
            eprintln!(
                "Unknown check: {}. Valid: plist | secrets | markdown",
                check
            );
            std::process::exit(1);
        }
    }

    if has_error {
        std::process::exit(1);
    }

    // Inner helpers
    fn check_plist(path: &str) -> bool {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return true,
        };

        println!("  Checking {}...", path);
        let mut valid = true;

        if !content.contains("{{USER}}") {
            eprintln!("❌ Missing {{USER}} placeholder in {}", path);
            valid = false;
        }
        if !content.contains("{{HOME}}") {
            eprintln!("❌ Missing {{HOME}} placeholder in {}", path);
            valid = false;
        }

        let temp_content = regex::Regex::new(r"\{\{[^}]*\}\}")
            .unwrap()
            .replace_all(&content, "1");

        // A missing `plutil` used to map to `.unwrap_or(false)`, i.e. "invalid
        // plist" — producing a false pre-commit failure instead of a skip. Probe it
        // first, matching how check_markdown handles a missing linter.
        if std::process::Command::new("plutil")
            .arg("-help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| !s.success())
            .unwrap_or(true)
        {
            println!("  ℹ plutil unavailable — skipping XML validation");
            return valid;
        }

        // Write to a private temp dir rather than a predictable /tmp path, which
        // another local user could pre-create as a symlink. This runs from a
        // pre-commit hook.
        let temp_dir = std::env::temp_dir().join(format!("mac-setup-audit-{}", std::process::id()));
        if std::fs::create_dir_all(&temp_dir).is_err() {
            println!("  ℹ could not create temp dir — skipping XML validation");
            return valid;
        }
        let temp_path = temp_dir.join("template.plist");
        if std::fs::write(&temp_path, temp_content.as_bytes()).is_err() {
            let _ = std::fs::remove_dir(&temp_dir);
            println!("  ℹ could not write temp plist — skipping XML validation");
            return valid;
        }

        let status = std::process::Command::new("plutil")
            .arg("-lint")
            .arg(&temp_path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let _ = std::fs::remove_file(&temp_path);
        let _ = std::fs::remove_dir(&temp_dir);

        if !status {
            eprintln!("❌ Invalid XML structure in {}", path);
            valid = false;
        }

        valid
    }

    fn check_secrets(path: &str) -> bool {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return true,
        };

        let patterns: &[(&str, &str)] = &[
            ("GitHub PAT", r"github_pat_[a-zA-Z0-9_]+"),
            ("GitHub Token", r"ghp_[a-zA-Z0-9]+"),
            ("AWS Key", r"AKIA[0-9A-Z]{16}"),
            ("Stripe SK", r"sk_live_[a-zA-Z0-9]+"),
            ("Stripe PK", r"pk_live_[a-zA-Z0-9]+"),
            (
                "Private Key",
                r"-----BEGIN (RSA|DSA|EC|OPENSSH) PRIVATE KEY-----",
            ),
            (
                "Password Assignment",
                r"password\s*=\s*['\x22][^'\x22]+['\x22]",
            ),
            (
                "API Key Assignment",
                r"api_key\s*=\s*['\x22][^'\x22]+['\x22]",
            ),
        ];

        let mut valid = true;
        for (name, pattern) in patterns.iter() {
            let re = regex::Regex::new(pattern).unwrap();
            if re.is_match(&content) {
                eprintln!("❌ Potential secret found in {}: {}", path, name);
                valid = false;
            }
        }
        valid
    }

    fn check_markdown(path: &str) -> bool {
        let available = std::process::Command::new("which")
            .arg("markdownlint")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !available {
            println!("⚠️  markdownlint not installed, skipping...");
            return true;
        }

        println!("  Checking {}...", path);
        std::process::Command::new("markdownlint")
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

// ============================================================================
// DISK SPACE HELPER
// ============================================================================

/// Log the current disk space available on `/`.
///
/// Called from both `run_update` and `run_fix` to avoid duplicating the
/// `df -h /` + column-index logic in two places.
fn log_disk_space(config: &Config) {
    if let Ok(output) = std::process::Command::new("df").args(["-h", "/"]).output() {
        let df_output = String::from_utf8_lossy(&output.stdout);
        for line in df_output.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                log(&format!("\n💾 Disk space available: {}", parts[3]), config);
            }
        }
    }
}

// ============================================================================
// SETUP (dev environment)
// ============================================================================

fn run_setup(config: &Config) {
    use std::process::Command;

    log("🔧 Setting up development environment...", config);

    // Check stout
    if !Command::new("which")
        .arg("stout")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        eprintln!("❌ stout not found. Run `mac-setup provision` first.");
        std::process::exit(1);
    }

    // Install lefthook
    if Command::new("which").arg("lefthook").status().is_err() {
        log("📦 Installing lefthook...", config);
        match Command::new(stout_cmd())
            .args(["install", "lefthook"])
            .status()
        {
            Ok(s) if !s.success() => log_error("Failed to install lefthook", config),
            Err(e) => log_error(
                &format!("Failed to run stout to install lefthook: {}", e),
                config,
            ),
            _ => {}
        }
    } else {
        log("✅ lefthook already installed", config);
    }

    log("\n📦 Installing optional development tools...", config);

    // Check Cargo
    if Command::new("which").arg("cargo").status().is_err() {
        log(
            "  ⚠️  Rust/Cargo not found. Check https://rust-lang.org",
            config,
        );
    } else {
        log("  ✅ Cargo found. Installing clippy/fmt...", config);
        let _ = Command::new("rustup")
            .args(["component", "add", "clippy", "rustfmt"])
            .status();
    }

    // Install hooks
    log("\n🪝 Installing git hooks...", config);
    if let Err(e) = Command::new("lefthook").arg("install").status() {
        log_error(&format!("Failed to install git hooks: {}", e), config);
    }

    log("\n✅ Development environment ready!", config);
    log("\nAvailable commands:", config);
    log("  lefthook run pre-commit  - Run pre-commit checks", config);
    log("  lefthook run pre-push    - Run pre-push tests", config);
}

// ============================================================================
// CLEANUP (standalone)
// ============================================================================

fn run_cleanup(config: &Config, aggressive: bool, dry_run: bool) {
    // Set the global dry-run flag so the low-level helpers can skip removals
    let _ = DRY_RUN.set(dry_run);

    log(
        &format!(
            "🧹 Running {} cleanup...",
            if aggressive {
                "aggressive"
            } else {
                "comprehensive"
            }
        ),
        config,
    );

    let stats = cleanup::comprehensive_cleanup(config, aggressive);
    log(&format!("{}", stats), config);

    // `stats.errors` used to be ignored entirely, so a run that recorded a problem
    // still reported success.
    if stats.errors.is_empty() {
        return;
    }
    for err in &stats.errors {
        log_error(err, config);
    }
    std::process::exit(1);
}

// ============================================================================
// INSTALL (launchd agent)
// ============================================================================

fn run_install(config: &Config) {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    log("==========================================", config);
    log("mac-setup — Launchd Agent Installer", config);
    log("==========================================", config);

    let user = env::var("USER").unwrap_or_default();
    let home = env::var("HOME").unwrap_or_default();
    if user.is_empty() || home.is_empty() {
        log_error(
            "USER or HOME is not set — cannot determine install paths",
            config,
        );
        std::process::exit(1);
    }

    log(&format!("Installing for user: {}", user), config);
    log(&format!("Home directory: {}", home), config);

    // Check stout
    if Command::new("which").arg("stout").status().is_err() {
        eprintln!("✗ stout not found. Run `mac-setup provision` first.");
        std::process::exit(1);
    }
    log("✓ stout found", config);

    // Build release binary
    log("Building mac-setup binary...", config);
    let status = match Command::new("cargo")
        .args(["build", "--release", "--bin", "mac-setup"])
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            log_error(&format!("Failed to execute cargo: {e}"), config);
            std::process::exit(1);
        }
    };

    if !status.success() {
        eprintln!("✗ Build failed.");
        std::process::exit(1);
    }

    // Install binary
    let scripts_dir = PathBuf::from(&home).join("Scripts");
    if !scripts_dir.exists() {
        log(&format!("Creating {}...", scripts_dir.display()), config);
        if let Err(e) = fs::create_dir_all(&scripts_dir) {
            log_error(&format!("Failed to create Scripts dir: {e}"), config);
            std::process::exit(1);
        }
    }

    // `target/release/mac-setup` and `config/*.plist.template` are both relative to
    // the repository root. `install` is the documented fresh-Mac bootstrap step and is
    // routinely run from elsewhere, so resolve the repo root from the current
    // executable's path instead of assuming the working directory.
    let repo_root = match repo_root_from_exe() {
        Some(root) => root,
        None => {
            log_error(
                "Could not locate the mac-setup repository root from the running binary",
                config,
            );
            std::process::exit(1);
        }
    };

    let target_bin = repo_root.join("target/release/mac-setup");
    let dest_bin = scripts_dir.join("mac-setup");

    log(
        &format!("Installing binary to {}...", dest_bin.display()),
        config,
    );
    if let Err(e) = fs::copy(&target_bin, &dest_bin) {
        log_error(&format!("Failed to copy binary: {e}"), config);
        std::process::exit(1);
    }

    // Read plist template
    let template_path = repo_root.join("config/com.USER.mac-setup.plist.template");
    let template_content = match fs::read_to_string(&template_path) {
        Ok(c) => c,
        Err(e) => {
            log_error(
                &format!("Failed to read {}: {e}", template_path.display()),
                config,
            );
            std::process::exit(1);
        }
    };

    // Schedule config
    let hour1 = env::var("MAC_SETUP_HOUR1").unwrap_or("9".to_string());
    let minute1 = env::var("MAC_SETUP_MINUTE1").unwrap_or("0".to_string());
    let hour2 = env::var("MAC_SETUP_HOUR2").unwrap_or("15".to_string());
    let minute2 = env::var("MAC_SETUP_MINUTE2").unwrap_or("0".to_string());
    let hour3 = env::var("MAC_SETUP_HOUR3").unwrap_or("21".to_string());
    let minute3 = env::var("MAC_SETUP_MINUTE3").unwrap_or("0".to_string());
    let nice = env::var("MAC_SETUP_NICE_LEVEL").unwrap_or("10".to_string());
    let throttle = env::var("MAC_SETUP_THROTTLE_INTERVAL").unwrap_or("300".to_string());
    let timeout = env::var("MAC_SETUP_EXIT_TIMEOUT").unwrap_or("7200".to_string());
    let min_disk = env::var("MAC_SETUP_MIN_DISK_SPACE_GB").unwrap_or("5".to_string());

    // `LOG_RETENTION_DAYS` is no longer templated: log retention is enforced by the
    // cleanup engine's 7-day sweep over ~/Library/Logs, not by an env var.
    let plist_content = template_content
        .replace("{{USER}}", &user)
        .replace("{{HOME}}", &home)
        .replace("{{HOUR1}}", &hour1)
        .replace("{{MINUTE1}}", &minute1)
        .replace("{{HOUR2}}", &hour2)
        .replace("{{MINUTE2}}", &minute2)
        .replace("{{HOUR3}}", &hour3)
        .replace("{{MINUTE3}}", &minute3)
        .replace("{{NICE_LEVEL}}", &nice)
        .replace("{{THROTTLE_INTERVAL}}", &throttle)
        .replace("{{EXIT_TIMEOUT}}", &timeout)
        .replace("{{MIN_DISK_SPACE_GB}}", &min_disk);

    // Write plist
    let plist_name = format!("com.{}.mac-setup.plist", user);
    let launch_agents = PathBuf::from(&home).join("Library/LaunchAgents");
    let plist_path = launch_agents.join(&plist_name);

    log(
        &format!("Writing plist to {}...", plist_path.display()),
        config,
    );
    // LaunchAgents does not exist on a machine that has never had an agent installed.
    if let Err(e) = fs::create_dir_all(&launch_agents) {
        log_error(
            &format!("Failed to create {}: {e}", launch_agents.display()),
            config,
        );
        std::process::exit(1);
    }
    if let Err(e) = fs::write(&plist_path, plist_content) {
        log_error(&format!("Failed to write plist: {e}"), config);
        std::process::exit(1);
    }

    // Reload launch agent
    log("Reloading launch agent...", config);
    // Inform the user if we're reloading an already-running agent.
    let already_loaded = Command::new("launchctl")
        .args(["list", &format!("com.{}.mac-setup", user)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if already_loaded {
        log(
            "ℹ Agent already loaded — reloading with new configuration",
            config,
        );
    }
    let _ = Command::new("launchctl")
        .args(["unload", &plist_path.to_string_lossy()])
        .output();
    let status = Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status();

    match status {
        Ok(s) if s.success() => log("✓ Launchd agent loaded", config),
        Ok(_) => eprintln!("⚠ Warning: launchctl load returned non-zero exit code"),
        Err(e) => log_error(&format!("Failed to run launchctl load: {e}"), config),
    }

    // Report the schedule that was actually installed, not a hardcoded string.
    let describe = |h: &str, m: &str| {
        let (Ok(hh), Ok(mm)) = (h.parse::<u32>(), m.parse::<u32>()) else {
            return format!("{h}:{m}");
        };
        let ampm = if hh < 12 { "AM" } else { "PM" };
        let h12 = match hh % 12 {
            0 => 12,
            n => n,
        };
        format!("{h12}:{mm:02} {ampm}")
    };

    log("\n==========================================", config);
    log("Installation Complete!", config);
    log("==========================================", config);
    log(&format!("\nBinary: {}", dest_bin.display()), config);
    log(&format!("Plist:  {}", plist_path.display()), config);
    log(
        &format!(
            "The update daemon runs automatically at {}, {} and {}.",
            describe(&hour1, &minute1),
            describe(&hour2, &minute2),
            describe(&hour3, &minute3)
        ),
        config,
    );
    log("Run manually: mac-setup update", config);
}

/// Walk up from the running executable to find the repository root.
///
/// Covers `target/debug/mac-setup` and `target/release/mac-setup`; falls back to the
/// current directory's ancestors so the command still works from a source checkout.
fn repo_root_from_exe() -> Option<std::path::PathBuf> {
    let start = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())?;

    start
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file() && dir.join("config").is_dir())
        .map(Path::to_path_buf)
}
