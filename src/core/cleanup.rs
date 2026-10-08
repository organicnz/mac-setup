//! Comprehensive cleanup module for development environments
//! Handles stout (formulae/casks), NPM, Cargo, and system caches

use super::utils::{self, Config};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn stout_cmd() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{}/.local/bin/stout", home);
    if Path::new(&local).exists() {
        return local;
    }
    "stout".to_string()
}

/// Comprehensive cleanup statistics
#[derive(Debug, Default)]
pub struct CleanupStats {
    pub bytes_freed: u64,
    pub files_removed: usize,
    pub dirs_removed: usize,
    pub stout_cache_freed: u64,
    pub npm_cache_freed: u64,
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
        if self.npm_cache_freed > 0 {
            writeln!(f, "   NPM: {}MB", self.npm_cache_freed / 1_000_000)?;
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
    cleanup_npm(config, &mut stats, &home);
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
        + stats.npm_cache_freed
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

fn cleanup_stout(config: &Config, stats: &mut CleanupStats, _aggressive: bool) {
    utils::log("  🦀 Cleaning stout...", config);

    // Always use --prune=all to remove all cached downloads (they can be re-downloaded)
    if let Ok(output) = Command::new(stout_cmd())
        .args(["cleanup", "--prune=all", "-s"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(bytes) = parse_size_from_output(&stdout) {
                stats.stout_cache_freed += bytes;
            }
        }
    }

    if let Ok(output) = Command::new(stout_cmd()).arg("autoremove").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stats.files_removed += stdout
                .lines()
                .filter(|l| l.starts_with("==> Uninstalling"))
                .count();
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();

    // Clean downloads directory completely - these are just cached .dmg/.pkg files
    let downloads_dir = format!("{}/Library/Caches/stout/downloads", home);
    if Path::new(&downloads_dir).exists() {
        let freed = get_dir_size(&downloads_dir);
        if let Ok(entries) = std::fs::read_dir(&downloads_dir) {
            for entry in entries.flatten() {
                let removed = if entry.path().is_file() {
                    std::fs::remove_file(entry.path()).is_ok()
                } else if entry.path().is_dir() {
                    std::fs::remove_dir_all(entry.path()).is_ok()
                } else {
                    false
                };
                if removed {
                    stats.files_removed += 1;
                }
            }
        }
        stats.stout_cache_freed += freed;
    }

    // Clean Cask cache completely as well
    let cask_dir = format!("{}/Library/Caches/stout/Cask", home);
    if Path::new(&cask_dir).exists() {
        let freed = get_dir_size(&cask_dir);
        if let Ok(entries) = std::fs::read_dir(&cask_dir) {
            for entry in entries.flatten() {
                let removed = if entry.path().is_file() {
                    std::fs::remove_file(entry.path()).is_ok()
                } else if entry.path().is_dir() {
                    std::fs::remove_dir_all(entry.path()).is_ok()
                } else {
                    false
                };
                if removed {
                    stats.files_removed += 1;
                }
            }
        }
        stats.stout_cache_freed += freed;
    }

    let log_dir = format!("{}/Library/Logs/stout", home);
    let freed = clean_old_files(&log_dir, 7, stats);
    stats.stout_cache_freed += freed;
}

fn cleanup_npm(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  📦 Cleaning NPM...", config);

    if !command_exists("npm") {
        return;
    }

    let _ = Command::new("npm")
        .args(["cache", "clean", "--force"])
        .output();

    let npm_caches = [
        format!("{}/.npm/_cacache", home),
        format!("{}/.npm/_logs", home),
        format!("{}/.npm/_npx", home),
    ];

    for cache in &npm_caches {
        let freed = clean_old_files(cache, 7, stats);
        stats.npm_cache_freed += freed;
    }

    let temp_node_modules = format!("{}/tmp/node_modules", home);
    if Path::new(&temp_node_modules).exists() {
        let freed = get_dir_size(&temp_node_modules);
        if std::fs::remove_dir_all(&temp_node_modules).is_ok() {
            stats.npm_cache_freed += freed;
            stats.dirs_removed += 1;
        }
    }
}

fn cleanup_cargo(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🦀 Cleaning Cargo...", config);

    if !command_exists("cargo") {
        return;
    }

    let registry_cache = format!("{}/.cargo/registry/cache", home);
    let freed = clean_old_files(&registry_cache, 30, stats);
    stats.cargo_cache_freed += freed;

    let registry_src = format!("{}/.cargo/registry/src", home);
    let freed = clean_old_files(&registry_src, 30, stats);
    stats.cargo_cache_freed += freed;

    let git_checkouts = format!("{}/.cargo/git/checkouts", home);
    let freed = clean_old_files(&git_checkouts, 30, stats);
    stats.cargo_cache_freed += freed;
}

fn cleanup_system_caches(config: &Config, stats: &mut CleanupStats, home: &str, aggressive: bool) {
    utils::log("  💻 Cleaning system caches...", config);

    let max_age = if aggressive { 1 } else { 7 };

    // Standard app caches
    let cache_dirs = [
        format!("{}/Library/Caches/com.apple.dt.Xcode", home),
        format!("{}/Library/Caches/org.swift.swiftpm", home),
        format!("{}/Library/Caches/com.googlecode.iterm2", home),
        format!("{}/Library/Caches/com.spotify.client", home),
        format!("{}/Library/Caches/com.microsoft.VSCode", home),
        format!("{}/Library/Caches/Google", home),
        format!("{}/.cache", home),
    ];

    for cache_dir in &cache_dirs {
        if Path::new(cache_dir).exists() {
            let freed = clean_old_files(cache_dir, max_age, stats);
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
        if aggressive {
            // In aggressive mode, clear all DerivedData
            let freed = clean_dir_contents(&derived_data, stats);
            stats.system_cache_freed += freed;
        } else {
            let freed = clean_old_files(&derived_data, 7, stats);
            stats.system_cache_freed += freed;
        }
    }

    let device_support = format!("{}/Library/Developer/Xcode/iOS DeviceSupport", home);
    if Path::new(&device_support).exists() {
        let freed = clean_old_dirs(&device_support, 30, stats);
        stats.system_cache_freed += freed;
    }

    // Xcode Archives older than 30 days
    let archives = format!("{}/Library/Developer/Xcode/Archives", home);
    if Path::new(&archives).exists() {
        let freed = clean_old_dirs(&archives, 30, stats);
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

    if !Path::new(dir).exists() {
        return 0;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let size = if entry.path().is_file() {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            } else if entry.path().is_dir() {
                get_dir_size(&entry.path().to_string_lossy())
            } else {
                0
            };

            let removed = if entry.path().is_file() {
                std::fs::remove_file(entry.path()).is_ok()
            } else if entry.path().is_dir() {
                std::fs::remove_dir_all(entry.path()).is_ok()
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
    if !command_exists("xcrun") {
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
        let freed = clean_dir_contents(&sim_caches, stats);
        stats.system_cache_freed += freed;
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
        let freed = clean_old_files(&gradle_caches, max_age, stats);
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
        let freed = clean_old_files(cache, 14, stats);
        stats.system_cache_freed += freed;
    }

    // Gradle caches
    let gradle_cache = format!("{}/.gradle/caches", home);
    let freed = clean_old_files(&gradle_cache, 30, stats);
    stats.system_cache_freed += freed;

    // CocoaPods cache
    let cocoapods_cache = format!("{}/Library/Caches/CocoaPods", home);
    let freed = clean_old_files(&cocoapods_cache, 14, stats);
    stats.system_cache_freed += freed;

    // Node.js related caches (can be fully cleaned)
    let node_caches = [
        format!("{}/Library/Caches/node-gyp", home), // node-gyp build cache
        format!("{}/Library/Caches/ms-playwright", home), // Playwright browsers
        format!("{}/Library/Caches/ms-playwright-go", home),
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
        let freed = clean_old_files(&go_cache, 14, stats);
        stats.system_cache_freed += freed;
    }

    // Claude CLI cache
    let claude_cache = format!("{}/Library/Caches/claude-cli-nodejs", home);
    if Path::new(&claude_cache).exists() {
        let freed = clean_old_files(&claude_cache, 7, stats);
        stats.system_cache_freed += freed;
    }

    if command_exists("yarn") {
        let _ = Command::new("yarn").args(["cache", "clean"]).output();
    }

    if command_exists("pnpm") {
        let _ = Command::new("pnpm").args(["store", "prune"]).output();
    }

    let bun_cache = format!("{}/.bun/install/cache", home);
    let freed = clean_old_files(&bun_cache, 14, stats);
    stats.system_cache_freed += freed;

    if command_exists("docker") {
        let _ = Command::new("docker")
            .args(["image", "prune", "-f"])
            .output();
        // Also prune build cache
        let _ = Command::new("docker")
            .args(["builder", "prune", "-f"])
            .output();
    }
}

/// Clean Time Machine local snapshots
fn cleanup_timemachine_snapshots(config: &Config, stats: &mut CleanupStats) {
    utils::log("  🕐 Cleaning Time Machine snapshots...", config);

    // List local snapshots - this works without sudo
    if let Ok(output) = Command::new("tmutil")
        .args(["listlocalsnapshots", "/"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            // Snapshot names start with "com.apple.TimeMachine"
            if line.starts_with("com.apple.TimeMachine") {
                // Try to delete - will fail silently without sudo
                let _ = Command::new("tmutil")
                    .args(["deletelocalsnapshots", line.trim()])
                    .output();
                stats.files_removed += 1;
            }
        }
    }
}

/// Clean Mail and Messages caches
fn cleanup_mail_messages(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  📧 Cleaning Mail & Messages caches...", config);

    // Mail caches - safe to clean
    let mail_caches = [
        format!("{}/Library/Caches/com.apple.mail", home),
        format!("{}/Library/Caches/com.apple.MobileSMS", home),
    ];

    for cache in &mail_caches {
        if Path::new(cache).exists() {
            let freed = clean_dir_contents(cache, stats);
            stats.system_cache_freed += freed;
        }
    }

    // Mail Downloads - clean old files (30+ days)
    let mail_downloads = format!("{}/Library/Mail Downloads", home);
    if Path::new(&mail_downloads).exists() {
        let freed = clean_old_files(&mail_downloads, 30, stats);
        stats.system_cache_freed += freed;
    }

    // Messages attachments - clean old files (30+ days)
    let messages_attachments = format!("{}/Library/Messages/Attachments", home);
    if Path::new(&messages_attachments).exists() {
        let freed = clean_old_files(&messages_attachments, 30, stats);
        stats.system_cache_freed += freed;
    }
}

/// Clean browser caches (Safari and third-party browsers)
fn cleanup_browser_caches(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🌐 Cleaning browser caches...", config);

    // Browser caches - can be fully cleaned (will regenerate)
    let browser_caches = [
        format!("{}/Library/Caches/com.apple.Safari", home),
        format!("{}/Library/Caches/com.brave.Browser", home),
        format!("{}/Library/Caches/com.google.Chrome", home),
        format!("{}/Library/Caches/org.mozilla.firefox", home),
        format!("{}/Library/Caches/com.microsoft.edgemac", home),
        format!("{}/Library/Caches/company.thebrowser.Browser", home), // Arc
    ];

    for cache in &browser_caches {
        if Path::new(cache).exists() {
            let freed = clean_dir_contents(cache, stats);
            stats.system_cache_freed += freed;
        }
    }

    // Safari local storage - clean old files (30+ days)
    let safari_storage = format!("{}/Library/Safari/LocalStorage", home);
    if Path::new(&safari_storage).exists() {
        let freed = clean_old_files(&safari_storage, 30, stats);
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

    // System-level diagnostic reports (may require permissions)
    let system_diagnostics = "/Library/Logs/DiagnosticReports";
    if Path::new(system_diagnostics).exists() {
        let freed = clean_dir_contents(system_diagnostics, stats);
        stats.system_cache_freed += freed;
    }

    // System logs - clean old files only (7+ days)
    let var_log = "/var/log";
    if Path::new(var_log).exists() {
        // Only clean old .log files, not current ones
        clean_old_log_files(var_log, 7, stats);
    }
}

/// Clean old/deprecated Swift packages and unused Xcode frameworks
fn cleanup_xcode_packages(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  🔨 Cleaning old Swift packages...", config);

    // Swift Package Manager - old cached packages (30+ days)
    let swiftpm_cache = format!("{}/Library/Caches/org.swift.swiftpm", home);
    if Path::new(&swiftpm_cache).exists() {
        let freed = clean_old_dirs(&swiftpm_cache, 30, stats);
        stats.system_cache_freed += freed;
    }

    // Swift PM repositories - old checkouts (60+ days)
    let swiftpm_repos = format!("{}/Library/org.swift.swiftpm/repositories", home);
    if Path::new(&swiftpm_repos).exists() {
        let freed = clean_old_dirs(&swiftpm_repos, 60, stats);
        stats.system_cache_freed += freed;
    }

    // Old module cache (30+ days) - precompiled Swift modules
    let module_cache = format!(
        "{}/Library/Developer/Xcode/DerivedData/ModuleCache.noindex",
        home
    );
    if Path::new(&module_cache).exists() {
        let freed = clean_old_dirs(&module_cache, 30, stats);
        stats.system_cache_freed += freed;
    }

    // Old Swift build artifacts in SourcePackages (60+ days)
    // These are resolved package dependencies that can be re-fetched
    if let Ok(entries) = std::fs::read_dir(format!("{}/Library/Developer/Xcode/DerivedData", home))
    {
        for entry in entries.flatten() {
            let source_packages = entry.path().join("SourcePackages");
            if source_packages.exists() {
                let freed = clean_old_dirs(&source_packages.to_string_lossy(), 60, stats);
                stats.system_cache_freed += freed;
            }
        }
    }
}

fn cleanup_old_logs(config: &Config, stats: &mut CleanupStats, home: &str) {
    utils::log("  📋 Cleaning old logs...", config);

    let log_dirs = [format!("{}/Library/Logs", home)];

    for log_dir in &log_dirs {
        if Path::new(log_dir).exists() {
            clean_old_log_files(log_dir, 7, stats);
        }
    }

    if let Some(log_parent) = config.log_file.parent() {
        clean_old_log_files(&log_parent.to_string_lossy(), 7, stats);
    }
}

fn cleanup_temp_files(config: &Config, stats: &mut CleanupStats) {
    utils::log("  🗑️  Cleaning temp files...", config);

    let temp_dirs = [
        "/tmp".to_string(),
        "/private/tmp".to_string(),
        std::env::var("TMPDIR").unwrap_or_default(),
    ];

    for temp_dir in &temp_dirs {
        if temp_dir.is_empty() || !Path::new(temp_dir).exists() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(temp_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed.as_secs() > 24 * 60 * 60 {
                                let size = if metadata.is_file() {
                                    metadata.len()
                                } else if metadata.is_dir() {
                                    get_dir_size(&entry.path().to_string_lossy())
                                } else {
                                    0
                                };

                                let removed = if metadata.is_file() {
                                    std::fs::remove_file(entry.path()).is_ok()
                                } else if metadata.is_dir() {
                                    std::fs::remove_dir_all(entry.path()).is_ok()
                                } else {
                                    false
                                };

                                if removed {
                                    stats.files_removed += 1;
                                    stats.system_cache_freed += size;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn cleanup_stale_locks(config: &Config, stats: &mut CleanupStats) {
    utils::log("  🔒 Cleaning stale locks...", config);

    let lock_patterns = [("/tmp", "stout"), ("/tmp", "brew-"), ("/tmp", ".npm-")];

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

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn parse_size_from_output(output: &str) -> Option<u64> {
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        for part in parts {
            if let Some(bytes) = parse_size_string(part) {
                return Some(bytes);
            }
        }
    }
    None
}

fn parse_size_string(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.ends_with("GB") || s.ends_with("G") {
        let num_str = s.trim_end_matches(|c: char| c.is_alphabetic());
        num_str
            .parse::<f64>()
            .ok()
            .map(|n| (n * 1_000_000_000.0) as u64)
    } else if s.ends_with("MB") || s.ends_with("M") {
        let num_str = s.trim_end_matches(|c: char| c.is_alphabetic());
        num_str
            .parse::<f64>()
            .ok()
            .map(|n| (n * 1_000_000.0) as u64)
    } else if s.ends_with("KB") || s.ends_with("K") {
        let num_str = s.trim_end_matches(|c: char| c.is_alphabetic());
        num_str.parse::<f64>().ok().map(|n| (n * 1_000.0) as u64)
    } else {
        None
    }
}

fn get_dir_size(path: &str) -> u64 {
    let mut size: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    size += metadata.len();
                } else if metadata.is_dir() {
                    size += get_dir_size(&entry.path().to_string_lossy());
                }
            }
        }
    }
    size
}

fn clean_old_files(dir: &str, max_age_days: u64, stats: &mut CleanupStats) -> u64 {
    let mut freed: u64 = 0;
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);

    if !Path::new(dir).exists() {
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
                                get_dir_size(&entry.path().to_string_lossy())
                            } else {
                                0
                            };

                            let removed = if metadata.is_file() {
                                std::fs::remove_file(entry.path()).is_ok()
                            } else if metadata.is_dir() {
                                std::fs::remove_dir_all(entry.path()).is_ok()
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

fn clean_old_dirs(dir: &str, max_age_days: u64, stats: &mut CleanupStats) -> u64 {
    let mut freed: u64 = 0;
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);

    if !Path::new(dir).exists() {
        return 0;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed > max_age {
                                let size = get_dir_size(&entry.path().to_string_lossy());
                                if std::fs::remove_dir_all(entry.path()).is_ok() {
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
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".log") || name.ends_with(".log.old") || name.ends_with(".log.gz") {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed > max_age && std::fs::remove_file(entry.path()).is_ok() {
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
