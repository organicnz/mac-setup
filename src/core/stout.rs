//! Contract layer for the `stout` CLI.
//!
//! stout is the only package manager this daemon shells out to, and three
//! separate bugs came from assuming it behaves like Homebrew: it has no
//! `--json` flag, it prints `All packages are up to date.` to stdout on
//! exit 0 (which the old code counted as a package to upgrade), and it
//! exits 0 even when a batch upgrade fails. Every assumption about stout's
//! output now lives here, so the next contract change is a one-file fix.
//!
//! All parsers are pure functions over the captured output so they are
//! covered by hermetic tests against real stout 0.2.2 output.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

/// The line stout prints to *stdout* — on exit 0 — when a query has no
/// results. The old code counted it as a package and upgraded the phantom
/// first word (`All`), so it must be matched before anything else.
const UP_TO_DATE: &str = "All packages are up to date.";

/// Whether an `outdated`/`upgrade` output means "nothing to do".
pub fn is_up_to_date(text: &str) -> bool {
    text.lines().any(|l| l.trim() == UP_TO_DATE)
}

/// Bullet artifacts from `stout info --cask <name>` output.
///
/// stout 0.2.2 has no `--json` flag, so the app path is scraped from the
/// human-readable block:
///
/// ```text
/// Artifacts:
///   • Slack.app
/// ```
///
/// Returns every bullet item (some casks ship binaries or pkgs too); the
/// caller decides which are `.app` bundles.
pub fn cask_artifacts(info_output: &str) -> Vec<String> {
    let mut in_section = false;
    let mut artifacts = Vec::new();
    for line in info_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_section {
                break;
            }
            continue;
        }
        if !in_section {
            in_section = trimmed == "Artifacts:";
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("• ") {
            artifacts.push(rest.trim().to_string());
        }
    }
    artifacts
}

/// The on-disk path for a `.app` artifact under `applications`, if it
/// exists. Non-`.app` artifacts are ignored.
pub fn app_path(artifact: &str, applications: &Path) -> Option<PathBuf> {
    if !artifact.ends_with(".app") {
        return None;
    }
    let path = applications.join(artifact);
    path.exists().then_some(path)
}

/// stout's install prefix. `stout --prefix` requires a value, so it cannot
/// discover the prefix; honour `STOUT_PREFIX` and probe the usual locations
/// instead. `prefix_from` is pure so it can be tested without touching the
/// real filesystem.
pub fn prefix() -> Option<PathBuf> {
    let env = std::env::var("STOUT_PREFIX").ok();
    prefix_from(&["/opt/homebrew", "/usr/local"], env.as_deref())
}

fn prefix_from(candidates: &[&str], env: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = env {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }
    candidates
        .iter()
        .map(Path::new)
        .find(|p| p.is_dir())
        .map(Path::to_path_buf)
}

/// SI-unit byte multipliers used in size parsing and display.
const KB: u64 = 1_000;
const MB: u64 = 1_000_000;
const GB: u64 = 1_000_000_000;
const TB: u64 = 1_000_000_000_000;

static FREED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:would\s+)?freed?\s+([0-9]+(?:\.[0-9]+)?)\s*(KB|MB|GB|TB)\s*\((\d+)\s*items?\)",
    )
    .expect("freed regex is valid")
});

/// `(bytes_freed, item_count)` from a `stout cleanup`/`autoremove` summary.
///
/// Matches both the dry-run form (`Dry run: Would free 3.28 MB (16 items)`)
/// and the applied form (`Freed 3.28 MB (16 items)`). Returns `None` when
/// the output carries no recognizable summary — the caller must treat that
/// as drift and fail loudly rather than silently report zero.
pub fn parse_freed(text: &str) -> Option<(u64, usize)> {
    FREED_RE.captures(text).map(|caps| {
        let num: f64 = caps[1].parse().unwrap_or(0.0);
        let bytes = match caps[2].to_ascii_uppercase().as_str() {
            "KB" => (num * KB as f64) as u64,
            "MB" => (num * MB as f64) as u64,
            "GB" => (num * GB as f64) as u64,
            _ => (num * TB as f64) as u64,
        };
        let count = caps[3].parse().unwrap_or(0);
        (bytes, count)
    })
}

/// Count the per-package removal lines in a `stout autoremove`
/// or `stout cleanup` output.
///
/// stout does not use Homebrew's `==> Uninstalling` prefix, so
/// counting that always returned zero. Count the removal markers
/// stout actually prints. Lines that do not match — including the
/// `No unused dependencies to remove.` no-op and the
/// `Dry run: Would free …` summary — are not counted, so an
/// unrecognized format reports zero rather than a false total.
pub fn count_removed_items(text: &str) -> usize {
    text.lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("Would remove:")
                || t.starts_with("Removed:")
                || t.starts_with("Removing:")
                || t.starts_with("Uninstalling:")
        })
        .count()
}

/// A single package that `stout upgrade` could not upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedPackage {
    pub name: String,
    pub reason: String,
}

/// Summary of a `stout upgrade` run, parsed from its output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeSummary {
    pub upgraded: usize,
    pub failed: Vec<FailedPackage>,
}

static UPGRADED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^Upgraded\s+(\d+)\s+packages?\s+in\s+[0-9.]+s(?:,\s*(\d+)\s+packages?\s+failed)?")
        .expect("upgraded regex is valid")
});

static FAILED_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^✗\s+(\S+)\s+-\s+(.+)$").expect("failed-line regex is valid"));

/// Parse a `stout upgrade` summary.
///
/// The summary line is `Upgraded 0 packages in 0.7s, 2 packages failed`
/// and each failure has its own `✗ <pkg> - <reason>` line. Because stout
/// exits 0 even when every package fails, the counts come from the output —
/// never from the exit status.
///
/// Returns `None` when no `Upgraded N packages` line is present, i.e. the
/// output does not match the expected shape at all.
pub fn parse_upgrade_summary(text: &str) -> Option<UpgradeSummary> {
    let mut summary = UpgradeSummary::default();
    let mut saw_summary = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(caps) = UPGRADED_RE.captures(trimmed) {
            saw_summary = true;
            summary.upgraded = caps[1].parse().unwrap_or(0);
        } else if let Some(caps) = FAILED_LINE_RE.captures(trimmed) {
            summary.failed.push(FailedPackage {
                name: caps[1].to_string(),
                reason: caps[2].trim().to_string(),
            });
        }
    }
    saw_summary.then_some(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sentinel ────────────────────────────────────────────────────────

    #[test]
    fn up_to_date_sentinel_is_detected() {
        assert!(is_up_to_date("All packages are up to date.\n"));
        assert!(is_up_to_date(
            "Checking for updates...\n\nAll packages are up to date.\n"
        ));
    }

    #[test]
    fn out_of_date_output_is_not_sentinel() {
        let text = "bat\ngh\n";
        assert!(!is_up_to_date(text));
    }

    // ── artifacts (real captured `stout info --cask slack`) ─────────────

    const SLACK_INFO: &str = "\
slack 4.52.178 (cask)
Slack
Team communication and collaboration software

Homepage:    https://slack.com/
Tap:         homebrew/cask
Type:        app

Artifacts:
  • Slack.app

URL:         https://downloads.slack-edge.com/desktop-releases/mac/arm64/4.52.178/Slack-4.52.178-macOS.dmg

Installed: No
";

    #[test]
    fn artifacts_are_scraped_from_the_text_block() {
        let artifacts = cask_artifacts(SLACK_INFO);
        assert_eq!(artifacts, vec!["Slack.app".to_string()]);
    }

    #[test]
    fn artifacts_handle_multiple_and_non_app_entries() {
        let text = "\
foo 1.0 (cask)
Foo

Artifacts:
  • Foo.app
  • Bar.app
  • baz_binary
  • Some.pkg

Installed: Yes
";
        let artifacts = cask_artifacts(text);
        assert_eq!(
            artifacts,
            vec![
                "Foo.app".to_string(),
                "Bar.app".to_string(),
                "baz_binary".to_string(),
                "Some.pkg".to_string(),
            ]
        );
    }

    #[test]
    fn artifacts_empty_when_section_absent() {
        assert!(cask_artifacts("slack 1.0 (cask)\nNo artifacts here.\n").is_empty());
    }

    // ── app_path ────────────────────────────────────────────────────────

    #[test]
    fn app_path_resolves_existing_bundle() {
        let dir = std::env::temp_dir().join("mac-setup-test-apppath");
        let _ = std::fs::create_dir_all(dir.join("Slack.app"));
        assert_eq!(app_path("Slack.app", &dir), Some(dir.join("Slack.app")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_path_returns_none_for_missing_bundle() {
        let dir = std::env::temp_dir().join("mac-setup-test-apppath-missing");
        assert_eq!(app_path("NotThere.app", &dir), None);
    }

    #[test]
    fn app_path_ignores_non_app_artifacts() {
        let dir = std::env::temp_dir();
        assert_eq!(app_path("baz_binary", &dir), None);
        assert_eq!(app_path("Some.pkg", &dir), None);
    }

    // ── prefix ──────────────────────────────────────────────────────────

    #[test]
    fn prefix_prefers_env_when_it_is_a_directory() {
        let dir = std::env::temp_dir().join("mac-setup-test-prefix");
        let _ = std::fs::create_dir_all(&dir);
        let got = prefix_from(
            &["/opt/homebrew", "/usr/local"],
            Some(dir.to_str().unwrap()),
        );
        assert_eq!(got, Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefix_falls_back_to_first_existing_candidate() {
        // Use two temp dirs so the test does not depend on the real machine.
        let base = std::env::temp_dir().join("mac-setup-test-prefix-cands");
        let a = base.join("a");
        let b = base.join("b");
        let _ = std::fs::create_dir_all(&a);
        let _ = std::fs::create_dir_all(&b);
        let candidates = [a.to_str().unwrap(), b.to_str().unwrap()];
        assert_eq!(prefix_from(&candidates, None), Some(a.clone()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prefix_returns_none_when_nothing_exists() {
        assert_eq!(
            prefix_from(&["/definitely/not/a/real/path", "/also/not/real"], None),
            None
        );
    }

    // ── parse_freed (real captured `stout cleanup --prune=7 -n -s`) ─────

    const CLEANUP_DRY_RUN: &str = "\
Cleaning up...
  Would remove: ca-certificates-2026-09-25-arm64_sonoma.tar.gz (109.55 KB)
  Would remove: oniguruma-6.9.10-arm64_sonoma.tar.gz (449.90 KB)
  Would remove: bat-0.26.1-arm64_sonoma.tar.gz (2.67 MB)
  Would remove: llhttp-9.4.3-arm64_sonoma.tar.gz (37.27 KB)

Dry run: Would free 3.28 MB (16 items)
";

    #[test]
    fn parse_freed_handles_dry_run_summary() {
        assert_eq!(parse_freed(CLEANUP_DRY_RUN), Some((3_280_000, 16)));
    }

    #[test]
    fn parse_freed_handles_applied_summary() {
        assert_eq!(
            parse_freed("Freed 1.2 GB (3 items)"),
            Some((1_200_000_000, 3))
        );
        assert_eq!(parse_freed("Freed 512 KB (1 item)"), Some((512_000, 1)));
    }

    #[test]
    fn parse_freed_returns_none_when_unrecognized() {
        assert_eq!(parse_freed("Cleaning up...\nNothing to do.\n"), None);
        assert_eq!(parse_freed(""), None);
    }

    // ── count_removed_items ─────────────────────────────────────────────

    #[test]
    fn count_removed_items_counts_removal_lines() {
        let text = "\
Cleaning up...
  Would remove: ca-certificates-2026-09-25-arm64_sonoma.tar.gz (109.55 KB)
  Would remove: oniguruma-6.9.10-arm64_sonoma.tar.gz (449.90 KB)
  Would remove: bat-0.26.1-arm64_sonoma.tar.gz (2.67 MB)

Dry run: Would free 3.28 MB (16 items)
";
        assert_eq!(count_removed_items(text), 3);
    }

    #[test]
    fn count_removed_items_handles_applied_uninstall_lines() {
        let text = "\
Checking for unused dependencies...

Uninstalling foo/bar (1.2.3)... done
Removed: foo/bar
";
        assert_eq!(count_removed_items(text), 1);
    }

    #[test]
    fn count_removed_items_ignores_no_op_summary() {
        // The no-op line contains "remove" but is not a removal action.
        let text = "Checking for unused dependencies...\n\nNo unused dependencies to remove.\n";
        assert_eq!(count_removed_items(text), 0);
        assert_eq!(
            count_removed_items("Dry run: Would free 3.28 MB (16 items)"),
            0
        );
        assert_eq!(count_removed_items(""), 0);
    }

    // ── parse_upgrade_summary (real captured `stout upgrade --formula`) ──

    const UPGRADE_PARTIAL_FAILURE: &str = "\
Checking for updates...

2 formulas can be upgraded:

  Package  Current     Latest
  ─────────────────────────────
  libssh2  1.11.1   → 1.11.1_7
  libgit2  1.9.7_1  → 1.9.7_2

Fetching package data...

! 2 packages could not be upgraded:
  ✗ libssh2 - no bottle for arm64_sonoma
  ✗ libgit2 - no bottle for arm64_sonoma

Upgraded 0 packages in 0.7s, 2 packages failed
";

    const UPGRADE_ALL_OK: &str = "\
Checking for updates...

Upgraded 3 packages in 1.2s
";

    #[test]
    fn parse_upgrade_summary_reports_partial_failure() {
        let summary = parse_upgrade_summary(UPGRADE_PARTIAL_FAILURE).unwrap();
        assert_eq!(summary.upgraded, 0);
        assert_eq!(summary.failed.len(), 2);
        assert_eq!(
            summary.failed[0],
            FailedPackage {
                name: "libssh2".to_string(),
                reason: "no bottle for arm64_sonoma".to_string(),
            }
        );
        assert_eq!(
            summary.failed[1],
            FailedPackage {
                name: "libgit2".to_string(),
                reason: "no bottle for arm64_sonoma".to_string(),
            }
        );
    }

    #[test]
    fn parse_upgrade_summary_handles_all_success() {
        let summary = parse_upgrade_summary(UPGRADE_ALL_OK).unwrap();
        assert_eq!(summary.upgraded, 3);
        assert!(summary.failed.is_empty());
    }

    #[test]
    fn parse_upgrade_summary_returns_none_without_summary_line() {
        assert_eq!(
            parse_upgrade_summary("Checking for updates...\n\nAll packages are up to date.\n"),
            None
        );
        assert_eq!(parse_upgrade_summary(""), None);
    }

    #[test]
    fn parse_upgrade_summary_tolerates_singular_package() {
        let summary = parse_upgrade_summary("Upgraded 1 package in 0.4s").unwrap();
        assert_eq!(summary.upgraded, 1);
    }
}
