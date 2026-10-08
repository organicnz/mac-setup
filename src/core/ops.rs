use super::{
    stats::CaskStats,
    utils::{self, Config},
};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

/// Returns the path to the stout binary, preferring ~/.local/bin/stout
fn stout_cmd() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{}/.local/bin/stout", home);
    if Path::new(&local).exists() {
        return local;
    }
    "stout".to_string()
}

// ============================================================================
// QUARANTINE REMOVAL HELPERS
// ============================================================================

/// Get the application path for a cask by querying stout info
fn get_cask_app_path(cask_name: &str) -> Option<String> {
    let output = Command::new(stout_cmd())
        .args(["info", "--cask", cask_name, "--json=v2"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json_str = String::from_utf8_lossy(&output.stdout);

    // Parse the JSON to find app artifacts
    // Looking for: "artifacts": [{"app": ["AppName.app"]}]
    // Simple parsing without serde - look for "app": ["Something.app"]
    if let Some(app_start) = json_str.find("\"app\"") {
        let remainder = &json_str[app_start..];
        if let Some(bracket_start) = remainder.find('[') {
            let after_bracket = &remainder[bracket_start + 1..];
            if let Some(quote_start) = after_bracket.find('"') {
                let after_quote = &after_bracket[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    let app_name = &after_quote[..quote_end];
                    if app_name.ends_with(".app") {
                        return Some(format!("/Applications/{}", app_name));
                    }
                }
            }
        }
    }

    None
}

/// Get the stout/Homebrew Cellar prefix directory (stout uses same layout)
fn get_stout_prefix() -> Option<String> {
    // Try `stout --prefix` first
    if let Ok(output) = Command::new(stout_cmd()).arg("--prefix").output() {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !prefix.is_empty() && std::path::Path::new(&prefix).exists() {
                return Some(prefix);
            }
        }
    }
    // Fallback to known paths
    for prefix in &["/opt/homebrew", "/usr/local"] {
        if std::path::Path::new(prefix).exists() {
            return Some(prefix.to_string());
        }
    }
    None
}

/// Remove quarantine attribute from an application
fn remove_quarantine(app_path: &str, config: &Config) {
    // Verify the path exists before attempting to remove quarantine
    if !std::path::Path::new(app_path).exists() {
        return;
    }

    match Command::new("xattr")
        .args(["-dr", "com.apple.quarantine", app_path])
        .output()
    {
        Ok(output) if output.status.success() => {
            utils::log(&format!("  ✓ Removed quarantine from {}", app_path), config);
        }
        _ => {
            // Silently ignore - quarantine may not exist or path may not be accessible
        }
    }
}

/// Remove quarantine attribute from ALL installed cask apps
/// This prevents Gatekeeper "Apple could not verify" warnings
pub fn remove_all_quarantine(config: &Config) -> usize {
    utils::log("🔓 Removing quarantine from all cask apps...", config);

    // Get list of all installed casks
    let output = match Command::new(stout_cmd()).args(["list", "--cask"]).output() {
        Ok(o) => o,
        Err(_) => {
            utils::log("  ⚠ Could not list installed casks", config);
            return 0;
        }
    };

    let casks: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    if casks.is_empty() {
        utils::log("  ℹ No casks installed", config);
        return 0;
    }

    let mut count = 0;
    for cask in &casks {
        if let Some(app_path) = get_cask_app_path(cask) {
            if std::path::Path::new(&app_path).exists() {
                // Check if app has quarantine attribute before removing
                let has_quarantine = Command::new("xattr")
                    .args(["-l", &app_path])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).contains("com.apple.quarantine"))
                    .unwrap_or(false);

                if has_quarantine {
                    match Command::new("xattr")
                        .args(["-dr", "com.apple.quarantine", &app_path])
                        .output()
                    {
                        Ok(o) if o.status.success() => {
                            utils::log(&format!("  ✓ {}", app_path), config);
                            count += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if count > 0 {
        utils::log(&format!("✓ Removed quarantine from {} apps", count), config);
    } else {
        utils::log("✓ No quarantine attributes found", config);
    }

    count
}

/// Remove quarantine attribute from ALL stout formula binaries
/// This prevents Gatekeeper "Apple could not verify" warnings for CLI tools
pub fn remove_all_formula_quarantine(config: &Config) -> usize {
    utils::log("🔓 Removing quarantine from formula binaries...", config);

    let prefix = match get_stout_prefix() {
        Some(p) => p,
        None => {
            utils::log("  ⚠ Could not determine stout prefix", config);
            return 0;
        }
    };

    let bin_dir = format!("{}/bin", prefix);
    let bin_path = std::path::Path::new(&bin_dir);

    if !bin_path.exists() {
        utils::log("  ⚠ stout bin directory not found", config);
        return 0;
    }

    let entries = match std::fs::read_dir(bin_path) {
        Ok(e) => e,
        Err(_) => {
            utils::log("  ⚠ Could not read stout bin directory", config);
            return 0;
        }
    };

    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();

        // Skip directories, only process files and symlinks
        if path.is_dir() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        // Check if file has quarantine attribute before removing
        let has_quarantine = Command::new("xattr")
            .args(["-l", &path_str])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("com.apple.quarantine"))
            .unwrap_or(false);

        if has_quarantine {
            match Command::new("xattr")
                .args(["-dr", "com.apple.quarantine", &path_str])
                .output()
            {
                Ok(o) if o.status.success() => {
                    if let Some(name) = path.file_name() {
                        utils::log(&format!("  ✓ {}", name.to_string_lossy()), config);
                    }
                    count += 1;
                }
                _ => {}
            }
        }
    }

    if count > 0 {
        utils::log(
            &format!("✓ Removed quarantine from {} formula binaries", count),
            config,
        );
    } else {
        utils::log(
            "✓ No quarantine attributes found on formula binaries",
            config,
        );
    }

    count
}

/// Attempt to recover an app that was disrupted during a failed upgrade
fn attempt_recovery(cask_name: &str, original_app_path: Option<&str>, config: &Config) -> bool {
    // Check if the app no longer exists (was moved during upgrade)
    if let Some(path) = original_app_path {
        if !std::path::Path::new(path).exists() {
            utils::log(
                &format!(
                    "  ⚠ {} was removed during upgrade, attempting recovery...",
                    path
                ),
                config,
            );

            // Try to reinstall the cask to restore the app
            match Command::new(stout_cmd())
                .args(["reinstall", "--cask", cask_name])
                .output()
            {
                Ok(output) if output.status.success() => {
                    utils::log(
                        &format!("  ✓ Recovered {} via reinstall", cask_name),
                        config,
                    );
                    // Remove quarantine from recovered app
                    if let Some(new_path) = get_cask_app_path(cask_name) {
                        remove_quarantine(&new_path, config);
                    }
                    return true;
                }
                _ => {
                    utils::log(
                        &format!(
                            "  ✗ Failed to recover {}. Manual reinstall: stout reinstall --cask {}",
                            cask_name, cask_name
                        ),
                        config,
                    );
                }
            }
        }
    }
    false
}

// ============================================================================
// TIMEOUT HELPER
// ============================================================================

/// Maximum time to wait for a single cask upgrade (30 minutes)
const CASK_UPGRADE_TIMEOUT_SECS: u64 = 1800;

/// Casks to skip entirely (manual installers, auth required, known issues)
const IGNORED_CASKS: &[&str] = &[
    "battle-net",
    "microsoft-edge",
    "visual-studio-code",
    "vmware-fusion",
    "windscribe",
];

/// Run a command with a timeout, returning the output or an error
fn run_with_timeout(cmd: &mut Command, timeout_secs: u64) -> Result<Output, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn: {}", e))?;

    match child.wait_timeout(Duration::from_secs(timeout_secs)) {
        Ok(Some(status)) => {
            // Process completed within timeout
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();

            if let Some(mut stdout_handle) = child.stdout.take() {
                stdout_handle.read_to_end(&mut stdout).ok();
            }
            if let Some(mut stderr_handle) = child.stderr.take() {
                stderr_handle.read_to_end(&mut stderr).ok();
            }

            Ok(Output {
                status,
                stdout,
                stderr,
            })
        }
        Ok(None) => {
            // Timeout exceeded - kill the process
            child.kill().ok();
            child.wait().ok(); // Reap the zombie process
            Err("Timeout exceeded".to_string())
        }
        Err(e) => Err(format!("Wait error: {}", e)),
    }
}

pub fn check_stout(config: &Config) -> bool {
    if Command::new("which")
        .arg("stout")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    // Check common paths (stout also installs to ~/.local/bin)
    let home = std::env::var("HOME").unwrap_or_default();
    let paths = [
        format!("{}/.local/bin/stout", home),
        "/usr/local/bin/stout".to_string(),
    ];
    for path in &paths {
        if std::path::Path::new(path).exists() {
            return true;
        }
    }

    utils::log_error("stout not found", config);
    false
}

pub fn update_stout(config: &Config) -> bool {
    utils::log("Updating stout index...", config);
    match Command::new(stout_cmd()).arg("update").status() {
        Ok(status) if status.success() => {
            utils::log("✓ stout index updated", config);
            true
        }
        _ => {
            utils::log_error("Failed to update stout index", config);
            false
        }
    }
}

pub fn upgrade_formulae(config: &Config) -> bool {
    utils::log("Checking for outdated formulae...", config);

    // Count outdated
    let outdated_count = Command::new(stout_cmd())
        .args(["outdated", "--formula"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0);

    if outdated_count == 0 {
        utils::log("✓ All formulae are up to date", config);
        return true;
    }

    utils::log(&format!("Upgrading {} formulae...", outdated_count), config);
    match Command::new(stout_cmd())
        .args(["upgrade", "--formula"])
        .status()
    {
        Ok(status) if status.success() => {
            utils::log("✓ Formulae upgraded successfully", config);
            true
        }
        _ => {
            utils::log_error("Failed to upgrade formulae", config);
            false
        }
    }
}

pub fn upgrade_casks(config: &Config) -> CaskStats {
    utils::log("Checking for outdated casks...", config);

    let mut stats = CaskStats::default();

    // Get list of outdated casks
    let outdated_output = Command::new(stout_cmd())
        .args(["outdated", "--cask", "--greedy"])
        .output();

    let outdated_casks: Vec<String> = match outdated_output {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect(),
        Err(_) => {
            utils::log("⚠ Could not check cask status, skipping", config);
            return stats;
        }
    };

    // Filter out ignored casks
    let outdated_casks: Vec<String> = outdated_casks
        .into_iter()
        .filter(|c| {
            let name = c.split_whitespace().next().unwrap_or(c);
            !IGNORED_CASKS.contains(&name)
        })
        .collect();

    if outdated_casks.is_empty() {
        utils::log("✓ All casks are up to date", config);
        return stats;
    }

    utils::log(
        &format!(
            "Found {} outdated casks, upgrading...",
            outdated_casks.len()
        ),
        config,
    );

    for cask in &outdated_casks {
        // Extract just the cask name (first word before any spaces)
        let cask_name = cask.split_whitespace().next().unwrap_or(cask);

        // Capture app path BEFORE upgrade to detect disruption if upgrade fails
        let original_app_path = get_cask_app_path(cask_name);

        // Log progress before starting the upgrade
        utils::log(&format!("  Upgrading {}...", cask_name), config);

        match run_with_timeout(
            Command::new(stout_cmd()).args(["upgrade", "--cask", cask_name]),
            CASK_UPGRADE_TIMEOUT_SECS,
        ) {
            Ok(output) if output.status.success() => {
                stats.upgraded += 1;
                utils::log(&format!("✓ Upgraded {}", cask_name), config);
                // Remove quarantine attribute to prevent Gatekeeper warnings
                if let Some(app_path) = get_cask_app_path(cask_name) {
                    remove_quarantine(&app_path, config);
                }
            }
            Err(e) if e.contains("Timeout") => {
                // Attempt recovery if app was disrupted during timed-out upgrade
                if attempt_recovery(cask_name, original_app_path.as_deref(), config) {
                    stats.recovered += 1;
                } else {
                    stats.skipped_timeout.push(cask_name.to_string());
                }
                utils::log(
                    &format!("⏱ Skipped {}: Upgrade timed out (>30min)", cask_name),
                    config,
                );
            }
            Err(e) => {
                // Attempt recovery if app was disrupted during failed upgrade
                if attempt_recovery(cask_name, original_app_path.as_deref(), config) {
                    stats.recovered += 1;
                } else {
                    stats.skipped_other.push(cask_name.to_string());
                }
                utils::log(&format!("⚠ Skipped {}: {}", cask_name, e), config);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr_lower = stderr.to_lowercase();

                // Categorize the skip reason
                if stderr_lower.contains("installer manual")
                    || stderr_lower.contains("manual installer")
                {
                    stats.skipped_manual.push(cask_name.to_string());
                    utils::log(
                        &format!("⚠ Skipped {}: Manual installer required", cask_name),
                        config,
                    );
                } else if stderr_lower.contains("is running")
                    || stderr_lower.contains("app is open")
                {
                    stats.skipped_running.push(cask_name.to_string());
                    utils::log(
                        &format!("⚠ Skipped {}: App is currently running", cask_name),
                        config,
                    );
                } else if stderr_lower.contains("authentication")
                    || stderr_lower.contains("password")
                    || stderr_lower.contains("disabled because it requires authentication")
                {
                    stats.skipped_auth.push(cask_name.to_string());
                    utils::log(
                        &format!("⚠ Skipped {}: Requires authentication", cask_name),
                        config,
                    );
                } else if stderr_lower.contains("definition is invalid")
                    || stderr_lower.contains("conflicts_with")
                {
                    stats.skipped_invalid.push(cask_name.to_string());
                    utils::log(
                        &format!("⚠ Skipped {}: Invalid cask definition", cask_name),
                        config,
                    );
                } else if stderr_lower.contains("app source")
                    || stderr_lower.contains("is not there")
                    || stderr_lower.contains("already an app at")
                {
                    stats.skipped_source_missing.push(cask_name.to_string());

                    // Try with --force flag for source issues
                    utils::log(
                        &format!(
                            "⚠ Skipped {}: App source issue, retrying with --force...",
                            cask_name
                        ),
                        config,
                    );
                    match run_with_timeout(
                        Command::new(stout_cmd()).args(["upgrade", "--cask", "--force", cask_name]),
                        CASK_UPGRADE_TIMEOUT_SECS,
                    ) {
                        Ok(retry_output) if retry_output.status.success() => {
                            stats.upgraded += 1;
                            stats.skipped_source_missing.pop(); // Remove from skipped list
                            utils::log(&format!("✓ Force-upgraded {}", cask_name), config);
                            // Remove quarantine attribute to prevent Gatekeeper warnings
                            if let Some(app_path) = get_cask_app_path(cask_name) {
                                remove_quarantine(&app_path, config);
                            }
                        }
                        Err(e) if e.contains("Timeout") => {
                            stats.skipped_source_missing.pop();
                            stats.skipped_timeout.push(cask_name.to_string());
                            utils::log(
                                &format!("  → Force upgrade timed out for {}", cask_name),
                                config,
                            );
                        }
                        _ => {
                            utils::log(
                                &format!("  → Force upgrade also failed for {}", cask_name),
                                config,
                            );
                        }
                    }
                } else {
                    // Truncate long error messages
                    let short_reason = if stderr.trim().len() > 100 {
                        format!("{}...", &stderr.trim()[0..100])
                    } else {
                        stderr.trim().to_string()
                    };

                    stats.skipped_other.push(cask_name.to_string());
                    utils::log(
                        &format!("⚠ Skipped {}: {}", cask_name, short_reason),
                        config,
                    );
                }
            }
        }
    }

    let total_skipped = stats.total_skipped();
    if total_skipped > 0 {
        utils::log(
            &format!(
                "✓ Casks: {} upgraded, {} skipped",
                stats.upgraded, total_skipped
            ),
            config,
        );
    } else {
        utils::log(
            &format!("✓ All {} casks upgraded successfully", stats.upgraded),
            config,
        );
    }

    stats
}

// ============================================================================
// HOUSEKEEPING FUNCTIONS
// ============================================================================

/// Statistics from housekeeping operations
#[derive(Debug, Default)]
pub struct HousekeepingStats {
    pub cache_cleared_bytes: u64,
    pub deps_removed: usize,
    pub logs_rotated: usize,
    pub locks_cleared: usize,
    pub stout_healthy: bool,
    pub doctor_warnings: usize,
    pub disk_available: String,
}

impl std::fmt::Display for HousekeepingStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache: {}MB freed, Deps removed: {}, Logs rotated: {}",
            self.cache_cleared_bytes / 1_000_000,
            self.deps_removed,
            self.logs_rotated
        )
    }
}

/// Pre-task housekeeping - cleanup and prepare the system
pub fn pre_housekeeping(config: &Config) -> HousekeepingStats {
    utils::log("📋 Running pre-task housekeeping...", config);
    let mut stats = HousekeepingStats::default();

    // 1. Clear stout cache to free space
    utils::log("  Clearing stout cache...", config);
    if let Ok(output) = Command::new(stout_cmd())
        .args(["cleanup", "--prune=30", "-s"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(bytes) = parse_bytes_freed(&stdout) {
                stats.cache_cleared_bytes = bytes;
            }
        }
    }

    // 2. Remove unused dependencies
    utils::log("  Removing unused dependencies...", config);
    if let Ok(output) = Command::new(stout_cmd()).arg("autoremove").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stats.deps_removed = stdout
                .lines()
                .filter(|l| l.starts_with("==> Uninstalling"))
                .count();
        }
    }

    // 3. Check stout health
    utils::log("  Checking stout health...", config);
    if let Ok(output) = Command::new(stout_cmd())
        .args(["doctor", "--quiet"])
        .output()
    {
        stats.doctor_warnings = String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|l| !l.is_empty())
            .count();
        stats.stout_healthy = output.status.success();
        if !stats.stout_healthy {
            utils::log(
                "  ⚠ stout doctor found issues (run 'stout doctor' for details)",
                config,
            );
        }
    }

    // 4. Clean NPM cache
    utils::log("  Cleaning NPM cache...", config);
    let _ = Command::new("npm")
        .args(["cache", "clean", "--force"])
        .output();

    // Log summary
    if stats.cache_cleared_bytes > 0 || stats.deps_removed > 0 {
        utils::log(
            &format!(
                "  ✓ Pre-housekeeping: freed ~{}MB, removed {} unused deps",
                stats.cache_cleared_bytes / 1_000_000,
                stats.deps_removed
            ),
            config,
        );
    } else {
        utils::log("  ✓ Pre-housekeeping completed (system was clean)", config);
    }

    stats
}

/// Post-task housekeeping - thorough cleanup after updates
pub fn post_housekeeping(config: &Config) -> HousekeepingStats {
    utils::log("📋 Running post-task housekeeping...", config);
    let mut stats = HousekeepingStats::default();

    // 1. Aggressive cache cleanup (older files)
    utils::log("  Deep cleaning stout cache...", config);
    if let Ok(output) = Command::new(stout_cmd())
        .args(["cleanup", "--prune=7", "-s"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(bytes) = parse_bytes_freed(&stdout) {
                stats.cache_cleared_bytes = bytes;
            }
        }
    }

    // 2. Remove unused dependencies again (updates may have orphaned some)
    utils::log("  Removing newly orphaned dependencies...", config);
    if let Ok(output) = Command::new(stout_cmd()).arg("autoremove").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stats.deps_removed = stdout
                .lines()
                .filter(|l| l.starts_with("==> Uninstalling"))
                .count();
        }
    }

    // 3. Clear download caches
    utils::log("  Clearing download caches...", config);
    clear_download_caches(&mut stats);

    // 4. Remove stale lock files
    utils::log("  Removing stale lock files...", config);
    clear_stale_locks(&mut stats);

    // 5. Clear old logs (keep last 7 days)
    utils::log("  Rotating old logs...", config);
    rotate_logs(config, &mut stats);

    // 6. Run garbage collection on stout package index
    utils::log("  Optimizing stout index...", config);
    let _ = Command::new(stout_cmd())
        .args(["update", "--auto-update"])
        .output();

    // Report final disk space
    if let Ok(output) = Command::new("df").args(["-h", "/"]).output() {
        let df_output = String::from_utf8_lossy(&output.stdout);
        for line in df_output.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                stats.disk_available = parts[3].to_string();
            }
        }
    }

    // Log summary
    utils::log(
        &format!(
            "  ✓ Post-housekeeping completed. Disk available: {}",
            if stats.disk_available.is_empty() {
                "unknown"
            } else {
                &stats.disk_available
            }
        ),
        config,
    );

    stats
}

/// Simple cleanup (for backward compatibility)
pub fn cleanup_stout_legacy(config: &Config) {
    utils::log("Running cleanup...", config);
    let _ = Command::new(stout_cmd())
        .args(["cleanup", "--prune=30", "-s"])
        .status();
    let _ = Command::new(stout_cmd()).arg("autoremove").status();
    utils::log("✓ Cleanup completed", config);
}

fn parse_bytes_freed(output: &str) -> Option<u64> {
    for line in output.lines() {
        if line.contains("freed") || line.contains("Removing") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for part in parts {
                if part.ends_with("GB") || part.ends_with("MB") || part.ends_with("KB") {
                    let num_str = part.trim_end_matches(|c: char| c.is_alphabetic());
                    if let Ok(num) = num_str.parse::<f64>() {
                        if part.ends_with("GB") {
                            return Some((num * 1_000_000_000.0) as u64);
                        } else if part.ends_with("MB") {
                            return Some((num * 1_000_000.0) as u64);
                        } else if part.ends_with("KB") {
                            return Some((num * 1_000.0) as u64);
                        }
                    }
                }
            }
        }
    }
    None
}

fn clear_download_caches(stats: &mut HousekeepingStats) {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return;
    }

    let cache_dir = format!("{}/Library/Caches/stout/downloads", home);
    if std::path::Path::new(&cache_dir).exists() {
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed.as_secs() > 7 * 24 * 60 * 60 {
                                let _ = std::fs::remove_file(entry.path());
                                stats.cache_cleared_bytes += metadata.len();
                            }
                        }
                    }
                }
            }
        }
    }
}

fn clear_stale_locks(stats: &mut HousekeepingStats) {
    let temp_path = std::path::Path::new("/tmp");
    if let Ok(entries) = std::fs::read_dir(temp_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if (name.starts_with("stout") || name.starts_with("brew-")) && name.ends_with(".lock") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed.as_secs() > 3600 {
                                let _ = std::fs::remove_file(entry.path());
                                stats.locks_cleared += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn rotate_logs(config: &Config, stats: &mut HousekeepingStats) {
    if let Some(log_dir) = config.log_file.parent() {
        if let Ok(entries) = std::fs::read_dir(log_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("mac-setup") && name.ends_with(".log.old") {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed.as_secs() > 7 * 24 * 60 * 60 {
                                    let _ = std::fs::remove_file(entry.path());
                                    stats.logs_rotated += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// MISE UPGRADE
// ============================================================================

pub fn upgrade_mise(config: &Config) -> bool {
    utils::log("🔧 Upgrading mise-managed runtimes and tools...", config);

    let home = std::env::var("HOME").unwrap_or_default();
    let mise = format!("{}/.local/bin/mise", home);
    let mise_bin = if std::path::Path::new(&mise).exists() {
        mise
    } else {
        "mise".to_string()
    };

    match Command::new(&mise_bin).args(["upgrade"]).status() {
        Ok(status) if status.success() => {
            utils::log("✓ mise tools upgraded", config);
            true
        }
        _ => {
            utils::log("⚠ mise upgrade completed with warnings", config);
            true // non-fatal
        }
    }
}

// ============================================================================
// PIPX UPGRADE
// ============================================================================

pub fn upgrade_pipx(config: &Config) {
    utils::log("🐍 Upgrading pipx apps...", config);

    // Find mise-managed python
    let home = std::env::var("HOME").unwrap_or_default();
    let python_candidates = [
        format!("{}/.local/share/mise/shims/python3", home),
        "python3".to_string(),
    ];

    let python = python_candidates
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .cloned()
        .unwrap_or("python3".to_string());

    // Check pipx is available
    let has_pipx = Command::new(&python)
        .args(["-m", "pipx", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !has_pipx {
        utils::log("  ℹ pipx not available, skipping", config);
        return;
    }

    match Command::new(&python)
        .args(["-m", "pipx", "upgrade-all"])
        .status()
    {
        Ok(status) if status.success() => {
            utils::log("✓ pipx apps upgraded", config);
        }
        _ => {
            utils::log("⚠ pipx upgrade completed with warnings", config);
        }
    }
}

// ============================================================================
// ============================================================================
// BUN UPDATE (replaces npm — bun is the JS runtime and package manager)
// ============================================================================

pub fn update_bun(config: &Config) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();

    // Find bun — check ~/.bun/bin first, then mise shim, then PATH
    let bun_candidates = [
        format!("{}/.bun/bin/bun", home),
        format!("{}/.local/share/mise/shims/bun", home),
    ];

    let bun_bin = bun_candidates
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .cloned()
        .unwrap_or_else(|| "bun".to_string());

    let bun_exists = Command::new(&bun_bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !bun_exists {
        utils::log("ℹ bun not installed, skipping bun global updates", config);
        return true;
    }

    utils::log("🐰 Updating bun globals...", config);

    match Command::new(&bun_bin).args(["update", "--global"]).output() {
        Ok(output) if output.status.success() => {
            utils::log("✓ Bun globals updated", config);
            true
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            utils::log("⚠ bun update completed with warnings:", config);
            for line in stderr.lines().take(5) {
                utils::log(&format!("  > {}", line), config);
            }
            true // non-fatal
        }
        Err(e) => {
            utils::log(&format!("⚠ bun update skipped: {}", e), config);
            true // non-fatal
        }
    }
}
