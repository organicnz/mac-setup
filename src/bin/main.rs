use clap::{Parser, Subcommand};
use mac_setup::commands::provision;
use mac_setup::core::utils::{log, log_error, send_notification, Config};
use mac_setup::{checks, cleanup, ops, stats};
use std::path::Path;

fn stout_cmd() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{}/.local/bin/stout", home);
    if Path::new(&local).exists() {
        return local;
    }
    "stout".to_string()
}

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

    // Ensure log directory exists
    if let Some(parent) = config.log_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match cli.command {
        Commands::Provision => run_provision(&config),
        Commands::Update => run_update(&config),
        Commands::Fix => run_fix(&config),
        Commands::Audit { check, files } => run_audit(&check, &files),
        Commands::Setup => run_setup(&config),
        Commands::Cleanup { aggressive } => run_cleanup(&config, aggressive),
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
        std::process::exit(1);
    }

    let mut update_stats = stats::UpdateStats::default();
    let mut overall_success = true;

    // Update operations
    log("\n📦 Running package updates...", config);

    if !ops::update_stout(config) {
        overall_success = false;
    }

    if !ops::upgrade_formulae(config) {
        overall_success = false;
    }

    update_stats.casks = ops::upgrade_casks(config);
    ops::remove_all_quarantine(config);
    ops::remove_all_formula_quarantine(config);

    // Upgrade mise-managed runtimes and tools
    if !ops::upgrade_mise(config) {
        overall_success = false;
    }

    // Upgrade pipx apps
    ops::upgrade_pipx(config);

    // Update bun globals
    if !ops::update_bun(config) {
        overall_success = false;
    }

    // Post-task cleanup
    log("\n", config);
    let post_cleanup_stats = cleanup::comprehensive_cleanup(config, false);

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
    if let Ok(output) = std::process::Command::new("df").args(["-h", "/"]).output() {
        let df_output = String::from_utf8_lossy(&output.stdout);
        for line in df_output.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                log(&format!("\n💾 Disk space available: {}", parts[3]), config);
            }
        }
    }

    log(
        "\n═══════════════════════════════════════════════════════════",
        config,
    );

    if overall_success {
        if update_stats.casks.has_actionable_items() {
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
    use std::io::{self, Write};
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

    log("\n[1/4] Cleaning up broken/missing casks...", config);
    for cask in casks_to_remove {
        let status = Command::new(stout_cmd())
            .args(["list", "--cask", cask])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if let Ok(s) = status {
            if s.success() {
                print!("  - Uninstalling {}... ", cask);
                io::stdout().flush().unwrap();

                let uninstall_status = Command::new(stout_cmd())
                    .args(["uninstall", "--cask", cask])
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .status();

                match uninstall_status {
                    Ok(us) if us.success() => println!("✓"),
                    _ => println!("✗ (Failed)"),
                }
            } else {
                println!("  - {} already removed.", cask);
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
            print!("  - Uninstalling flutter... ");
            io::stdout().flush().unwrap();
            let status = Command::new(stout_cmd())
                .args(["uninstall", "--cask", "flutter"])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .status();
            match status {
                Ok(us) if us.success() => println!("✓"),
                _ => println!("✗ (Failed)"),
            }
        } else {
            println!("  - Flutter already removed.");
        }
    }

    log("\n[3/4] Fixing invalid bun global package...", config);
    print!("  - Removing invalid package... ");
    io::stdout().flush().unwrap();
    let bun_status = Command::new("bun")
        .args(["remove", "--global", "@anthropic-ai/.claude-code-2DTsDk1V"])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status();
    match bun_status {
        Ok(s) if s.success() => println!("✓"),
        _ => println!("✗ (Failed or not present)"),
    }

    print!("  - Cleaning bun cache... ");
    io::stdout().flush().unwrap();
    let cache_status = Command::new("bun")
        .args(["pm", "cache", "rm"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match cache_status {
        Ok(s) if s.success() => println!("✓"),
        _ => println!("✗"),
    }

    log("\n[4/4] Final Housekeeping...", config);

    print!("  - Running stout cleanup... ");
    io::stdout().flush().unwrap();
    let cleanup = Command::new(stout_cmd())
        .args(["cleanup", "-s"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if cleanup.map(|s| s.success()).unwrap_or(false) {
        println!("✓");
    } else {
        println!("✗");
    }

    print!("  - Running stout autoremove... ");
    io::stdout().flush().unwrap();
    let autoremove = Command::new(stout_cmd())
        .arg("autoremove")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if autoremove.map(|s| s.success()).unwrap_or(false) {
        println!("✓");
    } else {
        println!("✗");
    }

    println!("  - Checking stout doctor...");
    let _ = Command::new(stout_cmd()).arg("doctor").status();

    log("\n==========================================", config);
    log("Fix Complete!", config);

    let df = Command::new("df").args(["-h", "/"]).output();
    if let Ok(out) = df {
        log("Current Disk Space:", config);
        log(&String::from_utf8_lossy(&out.stdout), config);
    }
    log("==========================================", config);
}

// ============================================================================
// AUDIT
// ============================================================================

fn run_audit(check: &str, files: &[String]) {
    use regex::Regex;
    use std::fs;
    use std::process::Command;

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
        let content = match fs::read_to_string(path) {
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

        let temp_content = Regex::new(r"\{\{[^}]*\}\}")
            .unwrap()
            .replace_all(&content, "1");
        let temp_path = format!("/tmp/temp_plist_{}", std::process::id());
        fs::write(&temp_path, temp_content.as_bytes()).unwrap();

        let status = Command::new("plutil")
            .arg("-lint")
            .arg(&temp_path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let _ = fs::remove_file(&temp_path);

        if !status {
            eprintln!("❌ Invalid XML structure in {}", path);
            valid = false;
        }

        valid
    }

    fn check_secrets(path: &str) -> bool {
        let content = match fs::read_to_string(path) {
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
            let re = Regex::new(pattern).unwrap();
            if re.is_match(&content) {
                eprintln!("❌ Potential secret found in {}: {}", path, name);
                valid = false;
            }
        }
        valid
    }

    fn check_markdown(path: &str) -> bool {
        let status = Command::new("which").arg("markdownlint").status();
        if status.is_err() || !status.unwrap().success() {
            println!("⚠️  markdownlint not installed, skipping...");
            return true;
        }

        println!("  Checking {}...", path);
        Command::new("markdownlint")
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
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
        let status = Command::new(stout_cmd())
            .args(["install", "lefthook"])
            .status()
            .unwrap();
        if !status.success() {
            eprintln!("Failed to install lefthook");
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
    let status = Command::new("lefthook").arg("install").status().unwrap();
    if !status.success() {
        eprintln!("Failed to install git hooks");
    }

    log("\n✅ Development environment ready!", config);
    log("\nAvailable commands:", config);
    log("  lefthook run pre-commit  - Run pre-commit checks", config);
    log("  lefthook run pre-push    - Run pre-push tests", config);
}

// ============================================================================
// CLEANUP (standalone)
// ============================================================================

fn run_cleanup(config: &Config, aggressive: bool) {
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

    let user = env::var("USER").expect("USER not set");
    let home = env::var("HOME").expect("HOME not set");

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
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", "mac-setup"])
        .status()
        .expect("Failed to execute cargo");

    if !status.success() {
        eprintln!("✗ Build failed.");
        std::process::exit(1);
    }

    // Install binary
    let scripts_dir = PathBuf::from(&home).join("Scripts");
    if !scripts_dir.exists() {
        log(&format!("Creating {}...", scripts_dir.display()), config);
        fs::create_dir_all(&scripts_dir).expect("Failed to create Scripts dir");
    }

    let target_bin = PathBuf::from("target/release/mac-setup");
    let dest_bin = scripts_dir.join("mac-setup");

    log(
        &format!("Installing binary to {}...", dest_bin.display()),
        config,
    );
    fs::copy(&target_bin, &dest_bin).expect("Failed to copy binary");

    // Read plist template
    let template_content = fs::read_to_string("config/com.USER.mac-setup.plist.template")
        .expect("Failed to read plist template");

    // Schedule config
    let hour1 = env::var("BREW_UPDATE_HOUR1").unwrap_or("9".to_string());
    let minute1 = env::var("BREW_UPDATE_MINUTE1").unwrap_or("0".to_string());
    let hour2 = env::var("BREW_UPDATE_HOUR2").unwrap_or("15".to_string());
    let minute2 = env::var("BREW_UPDATE_MINUTE2").unwrap_or("0".to_string());
    let hour3 = env::var("BREW_UPDATE_HOUR3").unwrap_or("21".to_string());
    let minute3 = env::var("BREW_UPDATE_MINUTE3").unwrap_or("0".to_string());
    let nice = env::var("BREW_UPDATE_NICE_LEVEL").unwrap_or("10".to_string());
    let throttle = env::var("BREW_UPDATE_THROTTLE_INTERVAL").unwrap_or("300".to_string());
    let timeout = env::var("BREW_UPDATE_EXIT_TIMEOUT").unwrap_or("7200".to_string());
    let retention = env::var("BREW_UPDATE_LOG_RETENTION_DAYS").unwrap_or("1".to_string());
    let min_disk = env::var("BREW_UPDATE_MIN_DISK_SPACE_GB").unwrap_or("5".to_string());

    // Replace template placeholders — binary path goes into the first <string> slot
    // The plist template already has <string>update</string> as a separate entry
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
        .replace("{{LOG_RETENTION_DAYS}}", &retention)
        .replace("{{MIN_DISK_SPACE_GB}}", &min_disk);

    // Write plist
    let plist_name = format!("com.{}.mac-setup.plist", user);
    let plist_path = PathBuf::from(&home)
        .join("Library/LaunchAgents")
        .join(&plist_name);

    log(
        &format!("Writing plist to {}...", plist_path.display()),
        config,
    );
    fs::write(&plist_path, plist_content).expect("Failed to write plist");

    // Reload launch agent
    log("Reloading launch agent...", config);
    let _ = Command::new("launchctl")
        .args(["unload", &plist_path.to_string_lossy()])
        .output();
    let status = Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()
        .expect("Failed to load plist");

    if status.success() {
        log("✓ Launchd agent loaded", config);
    } else {
        eprintln!("⚠ Warning: launchctl load returned non-zero exit code");
    }

    log("\n==========================================", config);
    log("Installation Complete!", config);
    log("==========================================", config);
    log(&format!("\nBinary: {}", dest_bin.display()), config);
    log(&format!("Plist:  {}", plist_path.display()), config);
    log(
        "\nThe update daemon runs automatically at 9 AM, 3 PM, and 9 PM.",
        config,
    );
    log("Run manually: mac-setup update", config);
}
