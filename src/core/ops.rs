use super::{
    stats::{CaskStats, ComponentStats},
    stout,
    utils::{self, home_path, stout_cmd, Config},
};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

// ============================================================================
// QUARANTINE REMOVAL HELPERS
// ============================================================================

/// Get the application path for a cask by querying stout info.
///
/// stout 0.2.2 has no `--json` flag, so the app bundle is scraped
/// from the human-readable `Artifacts:` block (see [`stout`]).
/// The old `--json=v2` call always exited 2, so this returned
/// `None` unconditionally and cask quarantine was never stripped.
fn get_cask_app_path(cask_name: &str) -> Option<String> {
    let output = Command::new(stout_cmd())
        .args(["info", "--cask", cask_name])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let info = String::from_utf8_lossy(&output.stdout);
    stout::cask_artifacts(&info)
        .into_iter()
        .find_map(|artifact| stout::app_path(&artifact, Path::new("/Applications")))
        .map(|p| p.to_string_lossy().into_owned())
}

/// Get the stout prefix directory.
///
/// `stout --prefix` requires a value, so it cannot be used to
/// discover the prefix (it exits 2). `stout::prefix` honours
/// `STOUT_PREFIX` and probes the usual locations instead.
fn get_stout_prefix() -> Option<String> {
    stout::prefix().map(|p| p.to_string_lossy().into_owned())
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

/// Count the paths under `target` that still carry quarantine, as reported by
/// `xattr -lr`. Used only for the log summary.
fn count_quarantined_paths(target: &str) -> usize {
    Command::new("xattr")
        .args(["-lr", target])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| l.contains("com.apple.quarantine"))
                .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
                .filter(|p| !p.is_empty())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        })
        .unwrap_or(0)
}

/// Remove quarantine attribute from ALL installed cask apps
/// This prevents Gatekeeper "Apple could not verify" warnings
pub fn remove_all_quarantine(config: &Config) -> usize {
    utils::log("🔓 Removing quarantine from all cask apps...", config);

    // Run `xattr -lr /Applications` once to find every quarantined file,
    // then group by .app bundle and run one `xattr -dr` per bundle.
    // This replaces the old per-cask `stout info` approach that spawned one
    // subprocess per installed cask even when none had quarantine.
    let output = match Command::new("xattr")
        .args(["-lr", "/Applications"])
        .output()
    {
        Ok(o) => o,
        Err(_) => {
            utils::log("  ⚠ Could not run xattr on /Applications", config);
            return 0;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Collect unique .app bundle paths that contain a quarantined file.
    // Each line from `xattr -lr` looks like:
    //   /Applications/Foo.app/Contents/…: com.apple.quarantine
    let mut quarantined_apps: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();

    for line in stdout.lines() {
        if !line.contains("com.apple.quarantine") {
            continue;
        }
        // Extract the path part (before the colon that precedes the xattr name).
        let path = line.split(':').next().unwrap_or("").trim();
        if path.is_empty() {
            continue;
        }
        // Walk up to find the .app bundle root.
        let mut parts = path.split('/');
        let mut bundle_path = String::from("/Applications");
        let mut found = false;
        for part in parts.by_ref() {
            if part.is_empty() || part == "Applications" {
                continue;
            }
            bundle_path = format!("{}/{}", bundle_path, part);
            if part.ends_with(".app") {
                found = true;
                break;
            }
        }
        if found && std::path::Path::new(&bundle_path).exists() {
            quarantined_apps.insert(bundle_path);
        }
    }

    if quarantined_apps.is_empty() {
        utils::log("✓ No quarantine attributes found", config);
        return 0;
    }

    let mut count = 0;
    for app_path in &quarantined_apps {
        remove_quarantine(app_path, config);
        count += 1;
    }

    utils::log(&format!("✓ Removed quarantine from {} apps", count), config);
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

    // Single recursive probe over the whole bin directory, then one recursive
    // removal — instead of one `xattr -l` and one `xattr -dr` per binary.
    let count = count_quarantined_paths(&bin_dir);
    if count == 0 {
        utils::log(
            "✓ No quarantine attributes found on formula binaries",
            config,
        );
        return 0;
    }

    match Command::new("xattr")
        .args(["-dr", "com.apple.quarantine", &bin_dir])
        .output()
    {
        Ok(o) if o.status.success() => {
            utils::log(
                &format!("✓ Removed quarantine from {} formula binaries", count),
                config,
            );
            count
        }
        _ => {
            utils::log(
                &format!("⚠ Could not strip quarantine from {} binaries", count),
                config,
            );
            0
        }
    }
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
/// Casks that are known to need a manual upgrade (installer prompts, license
/// acceptance) and so cannot be upgraded unattended.
///
/// This used to also silently skip `microsoft-edge`, `visual-studio-code`,
/// `vmware-fusion` and `windscribe` — four actively-maintained apps that were
/// installed but never updated, with no indication in the run summary. Those are
/// now upgraded like any other cask; if one genuinely cannot be upgraded
/// unattended it is categorized and reported instead of vanishing.
const MANUALLY_UPGRADABLE_CASKS: &[&str] = &["battle-net"];

/// Run a command with a timeout, returning the output or an error.
///
/// The child's stdout and stderr are drained by dedicated threads *while* it runs.
/// `wait_timeout` only polls `try_wait()` and does not read the pipes, so a child
/// that writes more than the ~64 KiB pipe buffer would block in `write(2)`, never
/// exit, and be killed at the full timeout — which is how chatty cask installers
/// used to be misreported as timeouts.
fn run_with_timeout(cmd: &mut Command, timeout_secs: u64) -> Result<Output, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn: {}", e))?;

    let stdout_reader = child.stdout.take().map(|mut h| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = h.read_to_end(&mut buf);
            buf
        })
    });

    let stderr_reader = child.stderr.take().map(|mut h| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = h.read_to_end(&mut buf);
            buf
        })
    });

    let status = match child.wait_timeout(Duration::from_secs(timeout_secs)) {
        Ok(Some(status)) => status,
        Ok(None) => {
            // Timeout exceeded - kill the process
            child.kill().ok();
            child.wait().ok(); // Reap the zombie process
            return Err("Timeout exceeded".to_string());
        }
        Err(e) => return Err(format!("Wait error: {}", e)),
    };

    // The child has exited, so the readers are guaranteed to see EOF.
    let stdout = stdout_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub fn check_stout(config: &Config) -> bool {
    let ok = Command::new(stout_cmd())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        utils::log_error("stout not found", config);
    }
    ok
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

/// Upgrade outdated formulae, returning real counts for the run summary.
///
/// `stout outdated` is unreliable — it reports "all up to date" even
/// when `stout upgrade` finds upgrades — so it is not used to decide
/// whether to run. `stout upgrade` is idempotent and prints the same
/// sentinel when there is nothing to do, so we always run it and parse
/// the real counts from its output. stout exits 0 even when a batch
/// upgrade fails, so the counts never come from the exit status.
pub fn upgrade_formulae(config: &Config) -> ComponentStats {
    let mut stats = ComponentStats::default();
    utils::log("Checking for outdated formulae...", config);

    let output = match Command::new(stout_cmd())
        .args(["upgrade", "--formula"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            utils::log_error(&format!("Could not run stout upgrade: {}", e), config);
            stats
                .errors
                .push("stout upgrade --formula did not run".to_string());
            return stats;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stout::is_up_to_date(&stdout) {
        utils::log("✓ All formulae are up to date", config);
        return stats;
    }

    match stout::parse_upgrade_summary(&stdout) {
        Some(summary) if summary.failed.is_empty() && summary.upgraded > 0 => {
            utils::log(
                &format!(
                    "✓ Formulae upgraded successfully ({} upgraded)",
                    summary.upgraded
                ),
                config,
            );
            stats.upgraded = summary.upgraded;
        }
        Some(summary) if summary.failed.is_empty() => {
            // Upgraded 0 with no failures: nothing was pending.
            utils::log("✓ All formulae are up to date", config);
        }
        Some(summary) => {
            stats.upgraded = summary.upgraded;
            stats.skipped = summary.failed.len();
            for f in &summary.failed {
                utils::log(&format!("  ✗ {} - {}", f.name, f.reason), config);
            }
            utils::log_error(
                &format!("{} formulae failed to upgrade", summary.failed.len()),
                config,
            );
        }
        None => {
            // The sentinel was handled above, so an unparseable output
            // means stout's contract changed or the run errored. Fail
            // loudly rather than report a clean success.
            if output.status.success() {
                utils::log_error(
                    "Could not read stout upgrade output (stout contract changed?)",
                    config,
                );
                stats
                    .errors
                    .push("stout upgrade output was unrecognized".to_string());
            } else {
                utils::log_error(
                    &format!(
                        "stout upgrade --formula failed (exit {})",
                        output.status.code().unwrap_or(-1)
                    ),
                    config,
                );
                stats
                    .errors
                    .push("stout upgrade --formula exited non-zero".to_string());
            }
        }
    }

    stats
}

pub fn upgrade_casks(config: &Config) -> CaskStats {
    utils::log("Checking for outdated casks...", config);

    let mut stats = CaskStats::default();

    // Get list of outdated casks
    let outdated_output = Command::new(stout_cmd())
        .args(["outdated", "--cask", "--greedy"])
        .output();

    let stdout = match outdated_output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(_) => {
            utils::log("⚠ Could not check cask status, skipping", config);
            return stats;
        }
    };

    // stout prints this sentinel to stdout on exit 0 when nothing is
    // outdated; the old code collected it as a cask named "All" and
    // ran `stout upgrade --cask All` on every run.
    if stout::is_up_to_date(&stdout) {
        utils::log("✓ All casks are up to date", config);
        return stats;
    }

    let outdated_casks: Vec<String> = stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    // Casks that cannot be upgraded unattended are reported rather than silently
    // dropped, so a skipped app is visible in the run summary.
    let outdated_casks: Vec<String> = outdated_casks
        .into_iter()
        .filter(|c| {
            let name = c.split_whitespace().next().unwrap_or(c);
            if MANUALLY_UPGRADABLE_CASKS.contains(&name) {
                stats.skipped_manual.push(name.to_string());
                utils::log(
                    &format!("  ℹ Skipping {}: manual upgrade required", name),
                    config,
                );
                return false;
            }
            true
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
        // Stop at a cask boundary once SIGTERM/SIGINT has arrived. The installer
        // mutates /Applications, so bailing out mid-cask is what we are avoiding —
        // but the cask in flight is allowed to finish so recovery can run.
        if utils::shutdown_requested() {
            stats.skipped_shutdown = outdated_casks.len();
            utils::log(
                &format!(
                    "⚠ Shutdown requested — {} casks not attempted this run",
                    stats.skipped_shutdown
                ),
                config,
            );
            break;
        }

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
            Ok(output) => {
                // stout exits 0 even when the upgrade failed, so
                // decide from the parsed output, never from the
                // exit status.
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                handle_cask_result(
                    cask_name,
                    &stdout,
                    &stderr,
                    original_app_path.as_deref(),
                    &mut stats,
                    config,
                );
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

/// Inspect the result of a single-cask `stout upgrade` and record it.
///
/// stout exits 0 even on failure, so the decision comes from the
/// parsed stdout (sentinel / `Upgraded N packages` / `✗ pkg - reason`),
/// falling back to stderr scraping only when stdout carries no
/// recognizable shape.
fn handle_cask_result(
    cask_name: &str,
    stdout: &str,
    stderr: &str,
    original_app_path: Option<&str>,
    stats: &mut CaskStats,
    config: &Config,
) {
    if let Some(summary) = stout::parse_upgrade_summary(stdout) {
        if summary.failed.is_empty() && summary.upgraded > 0 {
            stats.upgraded += 1;
            utils::log(&format!("✓ Upgraded {}", cask_name), config);
            // Reuse the path captured pre-upgrade: the upgrade does not
            // rename the .app, and re-querying cost an extra stout
            // process per cask.
            if let Some(app_path) = original_app_path {
                remove_quarantine(app_path, config);
            }
            return;
        }
        if let Some(failure) = summary.failed.first() {
            let reason = format!("{} - {}", failure.name, failure.reason);
            if record_cask_failure(cask_name, &reason, stats, config) {
                force_retry_cask(cask_name, stats, config);
            }
            return;
        }
        // Upgraded 0 with no failures: the cask was already current.
        utils::log(&format!("  ℹ {} already up to date", cask_name), config);
        return;
    }

    // Nothing recognizable on stdout. If stout printed the sentinel
    // the cask was current; otherwise fall back to the stderr-based
    // categorization.
    if stout::is_up_to_date(stdout) {
        utils::log(&format!("  ℹ {} already up to date", cask_name), config);
        return;
    }

    if record_cask_failure(cask_name, stderr, stats, config) {
        force_retry_cask(cask_name, stats, config);
    }
}

/// Categorize a cask upgrade failure into the right skip bucket.
///
/// Returns `true` when the failure is a source-missing issue that a
/// `--force` retry may resolve.
fn record_cask_failure(
    cask_name: &str,
    reason: &str,
    stats: &mut CaskStats,
    config: &Config,
) -> bool {
    let lower = reason.to_lowercase();

    if lower.contains("installer manual") || lower.contains("manual installer") {
        stats.skipped_manual.push(cask_name.to_string());
        utils::log(
            &format!("⚠ Skipped {}: Manual installer required", cask_name),
            config,
        );
    } else if lower.contains("is running") || lower.contains("app is open") {
        stats.skipped_running.push(cask_name.to_string());
        utils::log(
            &format!("⚠ Skipped {}: App is currently running", cask_name),
            config,
        );
    } else if lower.contains("authentication")
        || lower.contains("password")
        || lower.contains("disabled because it requires authentication")
    {
        stats.skipped_auth.push(cask_name.to_string());
        utils::log(
            &format!("⚠ Skipped {}: Requires authentication", cask_name),
            config,
        );
    } else if lower.contains("definition is invalid") || lower.contains("conflicts_with") {
        stats.skipped_invalid.push(cask_name.to_string());
        utils::log(
            &format!("⚠ Skipped {}: Invalid cask definition", cask_name),
            config,
        );
    } else if lower.contains("app source")
        || lower.contains("is not there")
        || lower.contains("already an app at")
    {
        stats.skipped_source_missing.push(cask_name.to_string());
        return true;
    } else {
        // Truncate long error messages
        let short_reason = if reason.trim().len() > 100 {
            format!("{}...", &reason.trim()[0..100])
        } else {
            reason.trim().to_string()
        };
        stats.skipped_other.push(cask_name.to_string());
        utils::log(
            &format!("⚠ Skipped {}: {}", cask_name, short_reason),
            config,
        );
    }

    false
}

/// Retry a source-missing cask upgrade with `--force`.
fn force_retry_cask(cask_name: &str, stats: &mut CaskStats, config: &Config) {
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
        Ok(output) => {
            // Same rule as the first attempt: the exit status is
            // worthless, so read the summary.
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let force_upgraded = stout::parse_upgrade_summary(&stdout)
                .is_some_and(|s| s.failed.is_empty() && s.upgraded > 0)
                || stout::is_up_to_date(&stdout);
            if force_upgraded {
                stats.upgraded += 1;
                stats.skipped_source_missing.pop(); // Remove from skipped list
                utils::log(&format!("✓ Force-upgraded {}", cask_name), config);
                // Remove quarantine attribute to prevent Gatekeeper warnings
                if let Some(app_path) = get_cask_app_path(cask_name) {
                    remove_quarantine(&app_path, config);
                }
            } else {
                utils::log(
                    &format!("  → Force upgrade also failed for {}", cask_name),
                    config,
                );
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
        Err(_) => {
            utils::log(
                &format!("  → Force upgrade also failed for {}", cask_name),
                config,
            );
        }
    }
}

// ============================================================================
// MISE UPGRADE
// ============================================================================

pub fn upgrade_mise(config: &Config) -> bool {
    utils::log("🔧 Upgrading mise-managed runtimes and tools...", config);

    let mise_path = home_path(".local/bin/mise");
    let mise_bin = if mise_path.exists() {
        mise_path.to_string_lossy().into_owned()
    } else {
        "mise".to_string()
    };

    // `mise upgrade` exits non-zero when *any* tool fails, so a partial failure is
    // reported honestly rather than being flattened into a `true`.
    match Command::new(&mise_bin).args(["upgrade"]).status() {
        Ok(status) if status.success() => {
            utils::log("✓ mise tools upgraded", config);
            true
        }
        Ok(status) => {
            utils::log(
                &format!(
                    "⚠ mise upgrade reported failures (exit {})",
                    status.code().unwrap_or(-1)
                ),
                config,
            );
            false
        }
        Err(e) => {
            utils::log(&format!("⚠ mise upgrade failed to run: {}", e), config);
            false
        }
    }
}

// ============================================================================
// PIPX UPGRADE
// ============================================================================

pub fn upgrade_pipx(config: &Config) -> bool {
    utils::log("🐍 Upgrading pipx apps...", config);

    // Find mise-managed python
    let python_candidates = [home_path(".local/share/mise/shims/python3")];

    let python = python_candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "python3".to_string());

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
        return true;
    }

    match Command::new(&python)
        .args(["-m", "pipx", "upgrade-all"])
        .status()
    {
        Ok(status) if status.success() => {
            utils::log("✓ pipx apps upgraded", config);
            true
        }
        Ok(status) => {
            utils::log(
                &format!(
                    "⚠ pipx upgrade reported failures (exit {})",
                    status.code().unwrap_or(-1)
                ),
                config,
            );
            false
        }
        Err(e) => {
            utils::log(&format!("⚠ pipx upgrade failed to run: {}", e), config);
            false
        }
    }
}

// ============================================================================
// ============================================================================
// BUN UPDATE (replaces npm — bun is the JS runtime and package manager)
// ============================================================================

/// Update bun global packages.
///
/// Returns a `ComponentStats` so the run summary can report a real number instead
/// of the hardcoded "Bun: 0 upgraded" this used to print on every run.
pub fn update_bun(config: &Config) -> ComponentStats {
    let mut stats = ComponentStats::default();

    // Find bun — check ~/.bun/bin first, then mise shim, then PATH
    let bun_candidates = [
        home_path(".bun/bin/bun"),
        home_path(".local/share/mise/shims/bun"),
    ];

    let bun_bin = bun_candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
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
        return stats;
    }

    utils::log("🐰 Updating bun globals...", config);

    match Command::new(&bun_bin).args(["update", "--global"]).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Count lines that bun emits for updated packages.
            // Fall back to 1 so the summary is never "0 upgraded" when bun
            // succeeds but changes its output format.
            let count = stdout
                .lines()
                .filter(|l| {
                    l.contains("updated")
                        || l.contains("installed")
                        || l.trim_start().starts_with("+ ")
                })
                .count();
            utils::log("✓ Bun globals updated", config);
            stats.upgraded = count.max(1);
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            utils::log("⚠ bun update completed with warnings:", config);
            for line in stderr.lines().take(5) {
                utils::log(&format!("  > {}", line), config);
            }
            stats.skipped = 1;
        }
        Err(e) => {
            utils::log(&format!("⚠ bun update skipped: {}", e), config);
            stats.skipped = 1;
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::run_with_timeout;
    use std::process::Command;

    /// Regression test for the pipe-buffer deadlock.
    ///
    /// `wait_timeout` does not drain the child's pipes, so any child writing more
    /// than the ~64 KiB pipe buffer would block forever in `write(2)` and be killed
    /// at the full timeout. A 256 KiB writer must therefore complete well inside the
    /// deadline rather than returning `Err("Timeout exceeded")`.
    #[test]
    fn run_with_timeout_handles_output_larger_than_pipe_buffer() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "dd if=/dev/zero bs=1024 count=256 2>/dev/null"]);

        let started = std::time::Instant::now();
        let result = run_with_timeout(&mut cmd, 15);
        let elapsed = started.elapsed();

        let output = result.expect("expected Ok, child should not have deadlocked");
        assert!(output.status.success(), "writer child should exit 0");
        assert_eq!(
            output.stdout.len(),
            256 * 1024,
            "all 256 KiB should be captured, not truncated"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "took {elapsed:?} — that suggests the pipes were not drained concurrently"
        );
    }

    #[test]
    fn run_with_timeout_reports_slow_child_as_timeout() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30"]);

        let result = run_with_timeout(&mut cmd, 1);

        assert_eq!(
            result.unwrap_err(),
            "Timeout exceeded",
            "a child exceeding its deadline must report a timeout"
        );
    }

    #[test]
    fn run_with_timeout_reports_missing_binary_as_spawn_error() {
        let mut cmd = Command::new("definitely-not-a-real-binary-xyz");
        let result = run_with_timeout(&mut cmd, 5);

        assert!(
            result.unwrap_err().starts_with("Failed to spawn:"),
            "expected a spawn error"
        );
    }
}
