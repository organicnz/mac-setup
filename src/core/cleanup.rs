//! Comprehensive cleanup module for development environments
//! Handles stout (formulae/casks), bun, Cargo, and system caches

use super::utils::{self, command_available, stout_cmd, Config};
use crate::core::stout;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

/// Controls whether cleanup passes actually delete anything.
/// Set once at startup from the CLI `--dry-run` flag; read by the
/// low-level `clean_old_*` and `clean_dir_contents` helpers so they
/// can skip removals without changing every function signature.
pub static DRY_RUN: OnceLock<bool> = OnceLock::new();

/// Comprehensive cleanup statistics
#[derive(Debug, Default)]
pub struct CleanupStats {
    pub bytes_freed: u64,
    pub files_removed: usize,
    pub dirs_removed: usize,
    pub stout_cache_freed: u64,
    pub bun_cache_freed: u64,
    pub cargo_cache_freed: u64,
    pub system_cache_freed: u64,
    pub logs_cleaned: usize,
    pub locks_removed: usize,
    pub errors: Vec<String>,
}

impl CleanupStats {
    pub fn total_mb_freed(&self) -> u64 {
        self.bytes_freed / 1_000_000
    }
}

impl std::fmt::Display for CleanupStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "📊 Cleanup Summary:")?;
        writeln!(f, "   Total freed: {}MB", self.total_mb_freed())?;
        writeln!(f, "   Files removed: {}", self.files_removed)?;
        if self.stout_cache_freed > 0 {
            writeln!(f, "   stout: {}MB", self.stout_cache_freed / 1_000_000)?;
        }
        if self.bun_cache_freed > 0 {
            writeln!(f, "   Bun: {}MB", self.bun_cache_freed / 1_000_000)?;
        }
        if self.cargo_cache_freed > 0 {
            writeln!(f, "   Cargo: {}MB", self.cargo_cache_freed / 1_000_000)?;
        }
        if self.system_cache_freed > 0 {
            writeln!(f, "   System: {}MB", self.system_cache_freed / 1_000_000)?;
        }
        if self.logs_cleaned > 0 {
            writeln!(f, "   Logs rotated: {}", self.logs_cleaned)?;
        }
        Ok(())
    }
}

/// Run comprehensive system cleanup
pub fn comprehensive_cleanup(config: &Config, aggressive: bool) -> CleanupStats {
    let mut stats = CleanupStats::default();
    let home = std::env::var("HOME").unwrap_or_default();

    if home.is_empty() {
        stats
            .errors
            .push("HOME environment variable not set".to_string());
        return stats;
    }

    utils::log("🧹 Running comprehensive cleanup...", config);

    cleanup_stout(config, &mut stats, aggressive);
    cleanup_bun(config, &mut stats, &home);
    cleanup_cargo(config, &mut stats, &home);
    cleanup_system_caches(config, &mut stats, &home, aggressive);
    cleanup_dev_caches(config, &mut stats, &home);
    cleanup_timemachine_snapshots(config, &mut stats);
    cleanup_mail_messages(config, &mut stats, &home);
    cleanup_browser_caches(config, &mut stats, &home);
    cleanup_system_diagnostics(config, &mut stats, &home);
    cleanup_xcode_packages(config, &mut stats, &home);
    cleanup_old_logs(config, &mut stats, &home);
    cleanup_temp_files(config, &mut stats);
    cleanup_stale_locks(config, &mut stats);

    stats.bytes_freed = stats.stout_cache_freed
        + stats.bun_cache_freed
        + stats.cargo_cache_freed
        + stats.system_cache_freed;

    utils::log(
        &format!(
            "✓ Cleanup complete: {}MB freed, {} files removed",
            stats.total_mb_freed(),
            stats.files_removed
        ),
        config,
    );

    stats
}

/// Clean only what the upgrade step itself created.
///
/// `comprehensive_cleanup` used to run twice per update with identical arguments,
/// which meant every stage — package-manager prunes, Time Machine snapshot
/// deletions, cache sweeps — ran twice 3x/day for no gain. The pre-flight sweep in
/// `run_update` already covers everything below; here we only drop caches the
/// upgrades refilled.
pub fn cleanup_after_upgrades(config: &Config) -> CleanupStats {
    let mut stats = CleanupStats::default();
    let home = std::env::var("HOME").unwrap_or_default();

    if home.is_empty() {
        stats
            .errors
            .push("HOME environment variable not set".to_string());
        return stats;
    }

    utils::log("🧹 Cleaning upgrade residue...", config);

    cleanup_stout(config, &mut stats, false);
    cleanup_bun(config, &mut stats, &home);

    stats.bytes_freed = stats.stout_cache_freed
        + stats.bun_cache_freed
        + stats.cargo_cache_freed
        + stats.system_cache_freed;

    utils::log(
        &format!(
            "✓ Cleanup complete: {}MB freed, {} files removed",
            stats.total_mb_freed(),
            stats.files_removed
        ),
        config,
    );

    stats
}

/// The `--prune` day count for `stout cleanup`.
///
/// aggressive → 0 ("all"); otherwise the `MAC_SETUP_PRUNE_DAYS` knob,
/// defaulting to 7. A separate function so the invalid-`all`-string
/// regression is unit-testable without shelling out.
fn prune_days_for(aggressive: bool, env_days: Option<&str>) -> u64 {
    if aggressive {
        return 0;
    }
    env_days.and_then(|v| v.trim().parse().ok()).unwrap_or(7)
}

fn cleanup_stout(config: &Config, stats: &mut CleanupStats, aggressive: bool) {
    utils::log("  🦀 Cleaning stout...", config);

    let dry = *DRY_RUN.get_or_init(|| false);

    // `stout cleanup` is the authoritative old-package cleaner: it prunes
    // downloads older than the prune window, and `--scrub` also drops
    // downloads for packages that are no longer installed. `--prune` takes a
    // day count where 0 means "all" — not the string "all", which stout
    // rejects, so aggressive mode used to prune nothing at all. Its own
    // summary is the single source of truth for bytes freed, so the
    // hand-rolled downloads/Cask deletion that used to follow here (and
    // double-counted the same bytes) is gone.
    let prune_days = prune_days_for(
        aggressive,
        std::env::var("MAC_SETUP_PRUNE_DAYS").ok().as_deref(),
    );
    let prune = format!("--prune={prune_days}");
    let mut cleanup_args: Vec<&str> = vec!["cleanup", &prune, "-s"];
    if dry {
        cleanup_args.push("-n");
    }
    if let Ok(output) = Command::new(stout_cmd()).args(&cleanup_args).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some((bytes, _)) = stout::parse_freed(&stdout) {
                stats.stout_cache_freed += bytes;
            }
        }
    }

    if let Ok(output) = Command::new(stout_cmd()).arg("autoremove").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // stout does not use Homebrew's "==> Uninstalling" prefix,
            // which made the old count always zero.
            stats.files_removed += stout::count_removed_items(&stdout);
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let log_dir = format!("{}/Library/Logs/stout", home);
    let freed = clean_old_files(Path::new(&log_dir), 7, stats);
    stats.stout_cache_freed += freed;
}

fn cleanup_bun(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🐰 Cleaning bun cache...", config);

    if command_available("bun") {
        let _ = Command::new("bun").args(["pm", "cache", "rm"]).output();
    }

    // Clean bun global cache directory
    let bun_cache = format!("{}/.bun/install/cache", home);
    let freed = clean_old_files(Path::new(&bun_cache), 14, stats);
    stats.bun_cache_freed += freed;

    // Residual npm directories from the pre-bun migration. Freed bytes are credited
    // to `system_cache_freed`, not `bun_cache_freed` — reporting them as "Bun" would
    // misattribute npm's cache to bun if anyone ever runs npm again.
    let npm_caches = [
        format!("{}/.npm/_cacache", home),
        format!("{}/.npm/_logs", home),
    ];
    for cache in &npm_caches {
        let freed = clean_old_files(Path::new(cache), 7, stats);
        stats.system_cache_freed += freed;
    }
}

fn cleanup_cargo(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🦀 Cleaning Cargo...", config);

    if !command_available("cargo") {
        return;
    }

    let registry_cache = format!("{}/.cargo/registry/cache", home);
    let freed = clean_old_files(Path::new(&registry_cache), 30, stats);
    stats.cargo_cache_freed += freed;

    let registry_src = format!("{}/.cargo/registry/src", home);
    let freed = clean_old_files(Path::new(&registry_src), 30, stats);
    stats.cargo_cache_freed += freed;

    let git_checkouts = format!("{}/.cargo/git/checkouts", home);
    let freed = clean_old_files(Path::new(&git_checkouts), 30, stats);
    stats.cargo_cache_freed += freed;
}

fn cleanup_system_caches(config: &Config, stats: &mut CleanupStats, home: &str, aggressive: bool) {
    utils::log("  💻 Cleaning system caches...", config);

    let max_age = if aggressive { 1 } else { 7 };

    // Standard app caches
    // `org.swift.swiftpm` is intentionally absent: it is swept at a 30-day threshold by
    // `cleanup_xcode_packages`. Including it here meant the 7-day pass emptied it
    // first, making that dedicated 30-day pass a guaranteed no-op.
    //
    // `~/.cache` is also intentionally absent. It is a shared convention directory
    // used by mise, bun, uv and pip — the very toolchains this daemon manages — so a
    // blanket age-based sweep can prune state another tool still considers live.
    let cache_dirs = [
        format!("{}/Library/Caches/com.apple.dt.Xcode", home),
        format!("{}/Library/Caches/com.googlecode.iterm2", home),
        format!("{}/Library/Caches/com.spotify.client", home),
        format!("{}/Library/Caches/com.microsoft.VSCode", home),
        format!("{}/Library/Caches/Google", home),
    ];

    for cache_dir in &cache_dirs {
        if Path::new(cache_dir).exists() {
            let freed = clean_old_files(Path::new(cache_dir), max_age, stats);
            stats.system_cache_freed += freed;
        }
    }

    // App caches that can be fully cleaned (will regenerate)
    let cleanable_caches = [
        format!("{}/Library/Caches/company.thebrowser.dia", home), // Dia browser
        format!("{}/Library/Caches/Dia", home),
        format!("{}/Library/Caches/loom-updater", home), // Loom updater cache
        format!("{}/Library/Caches/com.loom.desktop", home),
        format!("{}/Library/Caches/SiriTTS", home), // Siri text-to-speech cache
        format!("{}/Library/Caches/GeoServices", home), // Maps cache
    ];

    for cache_dir in &cleanable_caches {
        if Path::new(cache_dir).exists() {
            let freed = clean_dir_contents(cache_dir, stats);
            stats.system_cache_freed += freed;
        }
    }

    // Xcode DerivedData - clean completely in aggressive mode, or old projects otherwise
    let derived_data = format!("{}/Library/Developer/Xcode/DerivedData", home);
    if Path::new(&derived_data).exists() {
        // DerivedData is mutated by an in-flight Xcode build;
        // wiping it mid-build corrupts that build. Skip while
        // Xcode runs, matching the browser/mail liveness guards.
        if app_is_running("Xcode") {
            utils::log("  ℹ Skipping DerivedData — Xcode is running", config);
        } else if aggressive {
            // In aggressive mode, clear all DerivedData
            let freed = clean_dir_contents(&derived_data, stats);
            stats.system_cache_freed += freed;
        } else {
            let freed = clean_old_files(Path::new(&derived_data), 7, stats);
            stats.system_cache_freed += freed;
        }
    }

    let device_support = format!("{}/Library/Developer/Xcode/iOS DeviceSupport", home);
    if Path::new(&device_support).exists() {
        let freed = clean_old_dirs(Path::new(&device_support), 30, stats);
        stats.system_cache_freed += freed;
    }

    // Xcode Archives older than 30 days
    let archives = format!("{}/Library/Developer/Xcode/Archives", home);
    if Path::new(&archives).exists() {
        let freed = clean_old_dirs(Path::new(&archives), 30, stats);
        stats.system_cache_freed += freed;
    }

    // iOS Simulator cleanup
    cleanup_ios_simulators(config, stats);

    // Android SDK cleanup
    cleanup_android_sdk(config, stats, home, aggressive);
}

/// Clean all contents of a directory without removing the directory itself
fn clean_dir_contents(dir: &str, stats: &mut CleanupStats) -> u64 {
    let mut freed: u64 = 0;
    let dry = *DRY_RUN.get_or_init(|| false);

    if !Path::new(dir).exists() {
        return 0;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let size = if entry.path().is_file() {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            } else if entry.path().is_dir() {
                get_dir_size(&entry.path())
            } else {
                0
            };

            let removed = if !dry {
                if entry.path().is_file() {
                    std::fs::remove_file(entry.path()).is_ok()
                } else if entry.path().is_dir() {
                    std::fs::remove_dir_all(entry.path()).is_ok()
                } else {
                    false
                }
            } else {
                false
            };

            if removed {
                freed += size;
                stats.files_removed += 1;
            }
        }
    }

    freed
}

/// Clean unavailable iOS Simulators
fn cleanup_ios_simulators(config: &Config, stats: &mut CleanupStats) {
    if !command_available("xcrun") {
        return;
    }

    // Delete unavailable simulators
    if let Ok(output) = Command::new("xcrun")
        .args(["simctl", "delete", "unavailable"])
        .output()
    {
        if output.status.success() {
            utils::log("    Removed unavailable iOS simulators", config);
        }
    }

    // Erase all simulator data (in aggressive mode this could save GBs)
    // We don't do full erase as it's too aggressive, but we clean caches
    let home = std::env::var("HOME").unwrap_or_default();
    let sim_caches = format!("{}/Library/Developer/CoreSimulator/Caches", home);
    if Path::new(&sim_caches).exists() {
        // CoreSimulator state is live while Xcode or the
        // Simulator app run; wiping it mid-session breaks both.
        if app_is_running("Xcode") || app_is_running("Simulator") {
            utils::log(
                "    ℹ Skipping CoreSimulator caches — Simulator/Xcode is running",
                config,
            );
        } else {
            let freed = clean_dir_contents(&sim_caches, stats);
            stats.system_cache_freed += freed;
        }
    }
}

/// Clean Android SDK caches
fn cleanup_android_sdk(config: &Config, stats: &mut CleanupStats, home: &str, aggressive: bool) {
    let android_home = std::env::var("ANDROID_HOME")
        .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
        .unwrap_or_else(|_| format!("{}/Library/Android/sdk", home));

    if !Path::new(&android_home).exists() {
        return;
    }

    utils::log("    Cleaning Android SDK caches...", config);

    // Clean build cache
    let build_cache = format!("{}/.android/build-cache", home);
    if Path::new(&build_cache).exists() {
        let freed = clean_dir_contents(&build_cache, stats);
        stats.system_cache_freed += freed;
    }

    // Clean AVD cache (not the AVDs themselves)
    let avd_cache = format!("{}/.android/cache", home);
    if Path::new(&avd_cache).exists() {
        let freed = clean_dir_contents(&avd_cache, stats);
        stats.system_cache_freed += freed;
    }

    // Gradle build outputs in Android projects (be careful here)
    let gradle_caches = format!("{}/.gradle/caches/build-cache-1", home);
    if Path::new(&gradle_caches).exists() {
        let max_age = if aggressive { 7 } else { 30 };
        let freed = clean_old_files(Path::new(&gradle_caches), max_age, stats);
        stats.system_cache_freed += freed;
    }

    // Clean old Android SDK components that are no longer needed
    // This is conservative - only removes cache directories
    let sdk_cache_dirs = [
        format!("{}/cache", android_home),
        format!("{}/.temp", android_home),
    ];

    for cache_dir in &sdk_cache_dirs {
        if Path::new(cache_dir).exists() {
            let freed = clean_dir_contents(cache_dir, stats);
            stats.system_cache_freed += freed;
        }
    }
}

fn cleanup_dev_caches(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🛠️  Cleaning dev tool caches...", config);

    // VS Code caches
    let vscode_caches = [
        format!("{}/Library/Application Support/Code/Cache", home),
        format!("{}/Library/Application Support/Code/CachedData", home),
        format!("{}/Library/Application Support/Code/CachedExtensions", home),
        format!("{}/Library/Application Support/Code/logs", home),
    ];

    for cache in &vscode_caches {
        let freed = clean_old_files(Path::new(cache), 14, stats);
        stats.system_cache_freed += freed;
    }

    // Gradle manages its own dependency cache and re-resolves correctly when it is
    // missing; deleting `~/.gradle/caches` wholesale only forced a full
    // re-download. Age-filtered instead of a blanket wipe.
    let gradle_cache = format!("{}/.gradle/caches", home);
    let freed = clean_old_files(Path::new(&gradle_cache), 90, stats);
    stats.system_cache_freed += freed;

    // CocoaPods cache
    let cocoapods_cache = format!("{}/Library/Caches/CocoaPods", home);
    let freed = clean_old_files(Path::new(&cocoapods_cache), 14, stats);
    stats.system_cache_freed += freed;

    // Playwright's browser downloads are *installed artifacts*, not a cache:
    // `clean_dir_contents` here forced a full re-download of several hundred MB per
    // browser on the next `playwright install`. Age-filter instead.
    for cache in [
        format!("{}/Library/Caches/ms-playwright", home),
        format!("{}/Library/Caches/ms-playwright-go", home),
    ] {
        if Path::new(&cache).exists() {
            let freed = clean_old_files(Path::new(&cache), 60, stats);
            stats.system_cache_freed += freed;
        }
    }

    // Node build caches (genuinely regenerable, safe to wipe)
    let node_caches = [
        format!("{}/Library/Caches/node-gyp", home),
        format!("{}/Library/Caches/next-swc", home), // Next.js SWC cache
        format!("{}/Library/Caches/turbo", home),    // Turborepo cache
        format!("{}/Library/Caches/esbuild", home),  // esbuild cache
    ];

    for cache in &node_caches {
        if Path::new(cache).exists() {
            let freed = clean_dir_contents(cache, stats);
            stats.system_cache_freed += freed;
        }
    }

    // Python caches
    let python_caches = [
        format!("{}/Library/Caches/pip", home), // pip cache
        format!("{}/.cache/pip", home),         // pip cache (Linux-style)
        format!("{}/Library/Caches/uv", home),  // uv (fast pip) cache
    ];

    for cache in &python_caches {
        if Path::new(cache).exists() {
            let freed = clean_dir_contents(cache, stats);
            stats.system_cache_freed += freed;
        }
    }

    // Deno cache
    let deno_cache = format!("{}/Library/Caches/deno", home);
    if Path::new(&deno_cache).exists() {
        let freed = clean_dir_contents(&deno_cache, stats);
        stats.system_cache_freed += freed;
    }

    // Go module cache (can be large)
    let go_cache = format!("{}/Library/Caches/go-build", home);
    if Path::new(&go_cache).exists() {
        let freed = clean_old_files(Path::new(&go_cache), 14, stats);
        stats.system_cache_freed += freed;
    }

    // Claude CLI cache
    let claude_cache = format!("{}/Library/Caches/claude-cli-nodejs", home);
    if Path::new(&claude_cache).exists() {
        let freed = clean_old_files(Path::new(&claude_cache), 7, stats);
        stats.system_cache_freed += freed;
    }

    // `~/.bun/install/cache` is already handled by cleanup_npm above — cleaning it
    // again here found ~0 bytes and mis-attributed any hits to system_cache_freed.
    // Nothing further to do for bun here.
}

/// Time Machine local snapshots are intentionally left alone.
///
/// This used to call `tmutil deletelocalsnapshots` on every local snapshot 3x/day.
/// The old comment claimed it "will fail silently without sudo", which is wrong —
/// it succeeds without sudo for local snapshots on most configurations. That makes
/// local snapshots the *only* backups on a machine, and deleting them unattended is
/// not a decision a cleanup engine should make. `stats.files_removed` was also
/// incremented unconditionally, counting removals that never happened.
fn cleanup_timemachine_snapshots(config: &Config, _stats: &mut CleanupStats) {
    if command_available("tmutil") {
        if let Ok(output) = Command::new("tmutil")
            .args(["listlocalsnapshots", "/"])
            .output()
        {
            let count = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| l.starts_with("com.apple.TimeMachine"))
                .count();
            if count > 0 {
                utils::log(
                    &format!(
                        "  🕐 {} local Time Machine snapshot(s) — left untouched",
                        count
                    ),
                    config,
                );
            }
        }
    }
}

/// Whether an application is currently running.
///
/// The cask upgrader already refuses to upgrade a running app, but the cleanup
/// engine wiped Safari/Mail/Messages/Spotify/iTerm2/VS Code caches with no such
/// check — mutating a live app's cache directory 3x/day.
pub fn app_is_running(app_name: &str) -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg(app_name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Clean Mail and Messages caches
fn cleanup_mail_messages(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  📧 Cleaning Mail & Messages caches...", config);

    // Bundle-id suffix -> process name to liveness-check before wiping.
    let running_apps = [
        ("com.apple.mail", "Mail"),
        ("com.apple.MobileSMS", "Messages"),
    ];

    for (bundle_suffix, process) in running_apps {
        let cache = format!("{}/Library/Caches/{}", home, bundle_suffix);
        if !Path::new(&cache).exists() {
            continue;
        }
        if app_is_running(process) {
            utils::log(
                &format!("  ℹ Skipping {} — {} is running", bundle_suffix, process),
                config,
            );
            continue;
        }
        let freed = clean_dir_contents(&cache, stats);
        stats.system_cache_freed += freed;
    }

    // Mail Downloads - clean old files (30+ days)
    let mail_downloads = format!("{}/Library/Mail Downloads", home);
    if Path::new(&mail_downloads).exists() {
        let freed = clean_old_files(Path::new(&mail_downloads), 30, stats);
        stats.system_cache_freed += freed;
    }

    // Messages attachments - clean old files (30+ days)
    let messages_attachments = format!("{}/Library/Messages/Attachments", home);
    if Path::new(&messages_attachments).exists() {
        let freed = clean_old_files(Path::new(&messages_attachments), 30, stats);
        stats.system_cache_freed += freed;
    }
}

/// Clean browser caches (Safari and third-party browsers)
fn cleanup_browser_caches(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🌐 Cleaning browser caches...", config);

    // Browser caches regenerate on next launch, but only if the browser is closed.
    let browser_caches = [
        ("com.apple.Safari", "Safari"),
        ("com.brave.Browser", "Brave Browser"),
        ("com.google.Chrome", "Google Chrome"),
        ("org.mozilla.firefox", "firefox"),
        ("com.microsoft.edgemac", "Microsoft Edge"),
        ("company.thebrowser.Browser", "Arc"), // Arc
    ];

    for (bundle_suffix, process) in browser_caches {
        let cache = format!("{}/Library/Caches/{}", home, bundle_suffix);
        if !Path::new(&cache).exists() {
            continue;
        }
        if app_is_running(process) {
            utils::log(
                &format!("  ℹ Skipping {} — {} is running", bundle_suffix, process),
                config,
            );
            continue;
        }
        let freed = clean_dir_contents(&cache, stats);
        stats.system_cache_freed += freed;
    }

    // Safari local storage - clean old files (30+ days)
    let safari_storage = format!("{}/Library/Safari/LocalStorage", home);
    if Path::new(&safari_storage).exists() {
        let freed = clean_old_files(Path::new(&safari_storage), 30, stats);
        stats.system_cache_freed += freed;
    }
}

/// Clean system logs and diagnostic reports
fn cleanup_system_diagnostics(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  📊 Cleaning system diagnostics...", config);

    // User-level diagnostic reports - can be cleaned
    let user_diagnostics = [
        format!("{}/Library/Logs/DiagnosticReports", home),
        format!("{}/Library/Logs/CrashReporter", home),
        format!("{}/Library/Application Support/CrashReporter", home),
    ];

    for diag_dir in &user_diagnostics {
        if Path::new(diag_dir).exists() {
            let freed = clean_dir_contents(diag_dir, stats);
            stats.system_cache_freed += freed;
        }
    }

    // `/Library/Logs/DiagnosticReports` and `/var/log` are root-owned. Every removal
    // attempt there failed silently, so `clean_dir_contents` reported the apparent
    // size as "freed" while nothing was actually deleted — inflating the summary with
    // phantom wins forever. Skipped: it needs root, and the daemon runs unprivileged.
}

/// Clean old/deprecated Swift packages and unused Xcode frameworks
fn cleanup_xcode_packages(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🔨 Cleaning old Swift packages...", config);

    // Swift Package Manager - old cached packages (30+ days)
    let swiftpm_cache = format!("{}/Library/Caches/org.swift.swiftpm", home);
    if Path::new(&swiftpm_cache).exists() {
        let freed = clean_old_dirs(Path::new(&swiftpm_cache), 30, stats);
        stats.system_cache_freed += freed;
    }

    // Swift PM repositories - old checkouts (60+ days)
    let swiftpm_repos = format!("{}/Library/org.swift.swiftpm/repositories", home);
    if Path::new(&swiftpm_repos).exists() {
        let freed = clean_old_dirs(Path::new(&swiftpm_repos), 60, stats);
        stats.system_cache_freed += freed;
    }

    // Old module cache (30+ days) - precompiled Swift modules
    let module_cache = format!(
        "{}/Library/Developer/Xcode/DerivedData/ModuleCache.noindex",
        home
    );
    if Path::new(&module_cache).exists() {
        let freed = clean_old_dirs(Path::new(&module_cache), 30, stats);
        stats.system_cache_freed += freed;
    }

    // Old Swift build artifacts in SourcePackages (60+ days)
    // These are resolved package dependencies that can be re-fetched
    if let Ok(entries) = std::fs::read_dir(format!("{}/Library/Developer/Xcode/DerivedData", home))
    {
        for entry in entries.flatten() {
            let source_packages = entry.path().join("SourcePackages");
            if source_packages.exists() {
                let freed = clean_old_dirs(&source_packages, 60, stats);
                stats.system_cache_freed += freed;
            }
        }
    }
}

fn cleanup_old_logs(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  📋 Cleaning old logs...", config);

    let log_dirs = [format!("{}/Library/Logs", home)];

    // `config.log_file.parent()` is that same directory, so it used to be swept
    // twice per run. One pass is enough.
    for log_dir in &log_dirs {
        if Path::new(log_dir).exists() {
            clean_old_log_files(log_dir, 7, stats);
        }
    }
}

/// How long a temp entry must be idle before we consider it abandoned.
const TEMP_MAX_AGE_SECS: u64 = 24 * 60 * 60;

/// The current user's uid, derived from `$HOME`'s own metadata.
///
/// Reading it from a file we already know we own avoids both a `getuid` syscall
/// shim and a new dependency. Returns `None` if `$HOME` is unreadable, in which
/// case `temp_entry_is_removable` refuses to delete anything.
fn current_uid() -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let home = std::env::var("HOME").ok()?;
        std::fs::metadata(home).ok().map(|m| m.uid())
    }
    #[cfg(not(unix))]
    {
        Some(0)
    }
}

/// Decide whether a `/tmp` entry is safe to delete.
///
/// This used to delete anything older than 24h with no checks at all, which meant:
/// - other users' files, since macOS `/tmp` is world-writable,
/// - the daemon's own `/tmp/mac-setup.lock`, unlinked while `run_update` held an
///   exclusive flock on it. The flock survives on the orphaned inode, so a second
///   instance then created a *new* file, locked it successfully, and mutual
///   exclusion was silently lost.
///
/// Returns the entry's size when it is safe to remove.
fn temp_entry_is_removable(path: &Path, metadata: &std::fs::Metadata) -> Option<u64> {
    // Never delete symlinks: following them would let a link in /tmp redirect the
    // recursive delete to an arbitrary location elsewhere on disk.
    if metadata.file_type().is_symlink() {
        return None;
    }

    // Only touch what this user owns.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != current_uid()? {
            return None;
        }
    }

    // mac-setup's own lock file — we may be holding it right now.
    if path
        .file_name()
        .is_some_and(|n| n.to_string_lossy() == "mac-setup.lock")
    {
        return None;
    }

    // Idle for long enough to be considered abandoned.
    let modified = metadata.modified().ok()?;
    let elapsed = modified.elapsed().ok()?;
    if elapsed.as_secs() <= TEMP_MAX_AGE_SECS {
        return None;
    }

    // Don't recurse into a directory that holds a live socket — that is how tmux,
    // docker/podman and swiftpm keep their state, and deleting it out from under a
    // running process breaks it.
    if metadata.is_dir() && directory_holds_socket(path) {
        return None;
    }

    Some(if metadata.is_file() {
        metadata.len()
    } else if metadata.is_dir() {
        get_dir_size(path)
    } else {
        0
    })
}

/// Whether a directory tree contains a socket, i.e. looks live.
fn directory_holds_socket(dir: &Path) -> bool {
    fn walk(dir: &Path, depth: usize) -> bool {
        if depth == 0 {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                if meta.file_type().is_socket() || meta.file_type().is_fifo() {
                    return true;
                }
            }
            if meta.is_dir() && walk(&entry.path(), depth - 1) {
                return true;
            }
        }
        false
    }

    walk(dir, 3)
}

fn cleanup_temp_files(config: &Config, stats: &mut CleanupStats) {
    utils::log("  🗑️  Cleaning temp files...", config);

    let temp_dirs = [
        "/tmp".to_string(),
        "/private/tmp".to_string(),
        std::env::var("TMPDIR").unwrap_or_default(),
    ];

    // /tmp is a symlink to /private/tmp on macOS, so the two
    // names are the same directory. Canonicalize so each tree
    // is swept exactly once (and so a TMPDIR that lives under
    // /tmp is not swept twice).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for temp_dir in &temp_dirs {
        if temp_dir.is_empty() {
            continue;
        }

        let canonical = std::fs::canonicalize(temp_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| temp_dir.clone());
        if !seen.insert(canonical) {
            continue;
        }

        if !Path::new(temp_dir).exists() {
            continue;
        }

        let Ok(entries) = std::fs::read_dir(temp_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            let Some(size) = temp_entry_is_removable(&path, &metadata) else {
                continue;
            };

            let removed = if metadata.is_dir() {
                std::fs::remove_dir_all(&path).is_ok()
            } else {
                std::fs::remove_file(&path).is_ok()
            };

            if removed {
                stats.files_removed += 1;
                stats.system_cache_freed += size;
            }
        }
    }
}

fn cleanup_stale_locks(config: &Config, stats: &mut CleanupStats) {
    utils::log("  🔒 Cleaning stale locks...", config);

    // Stale lock files from tools that write into /tmp.
    let lock_patterns = [("/tmp", "stout"), ("/tmp", ".bun-")];

    for (dir, pattern) in &lock_patterns {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(pattern) && (name.ends_with(".lock") || name.contains("lock")) {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed.as_secs() > 2 * 60 * 60
                                    && std::fs::remove_file(entry.path()).is_ok()
                                {
                                    stats.locks_removed += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn get_dir_size(path: &Path) -> u64 {
    let mut size: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    size += metadata.len();
                } else if metadata.is_dir() {
                    size += get_dir_size(&entry.path());
                }
            }
        }
    }
    size
}

fn clean_old_files(dir: &Path, max_age_days: u64, stats: &mut CleanupStats) -> u64 {
    let mut freed: u64 = 0;
    let dry = *DRY_RUN.get_or_init(|| false);
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);

    if !dir.exists() {
        return 0;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        if elapsed > max_age {
                            let size = if metadata.is_file() {
                                metadata.len()
                            } else if metadata.is_dir() {
                                get_dir_size(&entry.path())
                            } else {
                                0
                            };

                            let removed = if !dry {
                                if metadata.is_file() {
                                    std::fs::remove_file(entry.path()).is_ok()
                                } else if metadata.is_dir() {
                                    std::fs::remove_dir_all(entry.path()).is_ok()
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            if removed {
                                freed += size;
                                stats.files_removed += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    freed
}

fn clean_old_dirs(dir: &Path, max_age_days: u64, stats: &mut CleanupStats) -> u64 {
    let mut freed: u64 = 0;
    let dry = *DRY_RUN.get_or_init(|| false);
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);

    if !dir.exists() {
        return 0;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed > max_age {
                                let size = get_dir_size(&entry.path());
                                if !dry && std::fs::remove_dir_all(entry.path()).is_ok() {
                                    freed += size;
                                    stats.dirs_removed += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    freed
}

fn clean_old_log_files(dir: &str, max_age_days: u64, stats: &mut CleanupStats) {
    let dry = *DRY_RUN.get_or_init(|| false);
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".log") || name.ends_with(".log.old") || name.ends_with(".log.gz") {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed > max_age
                                    && !dry
                                    && std::fs::remove_file(entry.path()).is_ok()
                                {
                                    stats.logs_cleaned += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_days_aggressive_is_zero() {
        // 0 means "all" to stout; the string "all" is rejected by stout,
        // which used to make aggressive mode prune nothing at all.
        assert_eq!(prune_days_for(true, Some("7")), 0);
        assert_eq!(prune_days_for(true, None), 0);
    }

    #[test]
    fn prune_days_non_aggressive_reads_knob() {
        assert_eq!(prune_days_for(false, Some("30")), 30);
        assert_eq!(prune_days_for(false, Some(" 14 ")), 14);
    }

    #[test]
    fn prune_days_defaults_when_knob_absent_or_invalid() {
        assert_eq!(prune_days_for(false, None), 7);
        assert_eq!(prune_days_for(false, Some("nonsense")), 7);
        assert_eq!(prune_days_for(false, Some("")), 7);
    }
}
