pub mod checks;
pub mod cleanup;
pub mod ops;
pub mod stats;
pub mod utils;

#[cfg(test)]
mod tests {
    use super::stats::*;
    use super::utils::*;
    use std::path::PathBuf;

    // ── Config ────────────────────────────────────────────────────────────────

    #[test]
    fn config_default_paths_are_set() {
        let config = Config::default();
        assert!(config.log_file.to_str().unwrap().contains("mac-setup.log"));
        assert!(config
            .error_log
            .to_str()
            .unwrap()
            .contains("mac-setup-error.log"));
        assert!(config
            .lock_file
            .to_str()
            .unwrap()
            .contains("mac-setup.lock"));
    }

    #[test]
    fn config_log_file_has_correct_suffix() {
        let config = Config::default();
        assert!(config
            .log_file
            .extension()
            .map(|e| e == "log")
            .unwrap_or(false));
    }

    // ── CaskStats ─────────────────────────────────────────────────────────────

    #[test]
    fn cask_stats_total_skipped_empty() {
        let stats = CaskStats::default();
        assert_eq!(stats.total_skipped(), 0);
    }

    #[test]
    fn cask_stats_total_skipped_counts_all_buckets() {
        let mut stats = CaskStats::default();
        stats.skipped_manual.push("a".into());
        stats.skipped_running.push("b".into());
        stats.skipped_auth.push("c".into());
        stats.skipped_invalid.push("d".into());
        stats.skipped_source_missing.push("e".into());
        stats.skipped_timeout.push("f".into());
        stats.skipped_other.push("g".into());
        assert_eq!(stats.total_skipped(), 7);
    }

    #[test]
    fn cask_stats_has_actionable_items_false_when_empty() {
        let stats = CaskStats::default();
        assert!(!stats.has_actionable_items());
    }

    #[test]
    fn cask_stats_has_actionable_items_true_when_running() {
        let mut stats = CaskStats::default();
        stats.skipped_running.push("slack".into());
        assert!(stats.has_actionable_items());
    }

    #[test]
    fn cask_stats_has_actionable_items_true_when_source_missing() {
        let mut stats = CaskStats::default();
        stats.skipped_source_missing.push("figma".into());
        assert!(stats.has_actionable_items());
    }

    // ── UpdateStats ───────────────────────────────────────────────────────────

    #[test]
    fn update_stats_default_is_zeroed() {
        let stats = UpdateStats::default();
        assert_eq!(stats.formulae.upgraded, 0);
        assert_eq!(stats.formulae.skipped, 0);
        assert_eq!(stats.npm.upgraded, 0);
        assert_eq!(stats.casks.total_skipped(), 0);
    }

    #[test]
    fn update_stats_display_does_not_panic() {
        let stats = UpdateStats::default();
        let output = format!("{}", stats);
        assert!(output.contains("Formulae"));
    }

    // ── ComponentStats ────────────────────────────────────────────────────────

    #[test]
    fn component_stats_default_zero() {
        let stats = ComponentStats::default();
        assert_eq!(stats.upgraded, 0);
        assert_eq!(stats.skipped, 0);
    }

    // ── CleanupStats ──────────────────────────────────────────────────────────

    #[test]
    fn cleanup_stats_total_mb_freed_zero() {
        use super::cleanup::CleanupStats;
        let stats = CleanupStats::default();
        assert_eq!(stats.total_mb_freed(), 0);
    }

    #[test]
    fn cleanup_stats_total_mb_freed_correct() {
        use super::cleanup::CleanupStats;
        let mut stats = CleanupStats::default();
        stats.bytes_freed = 5_000_000;
        assert_eq!(stats.total_mb_freed(), 5);
    }

    #[test]
    fn cleanup_stats_display_does_not_panic() {
        use super::cleanup::CleanupStats;
        let stats = CleanupStats::default();
        let output = format!("{}", stats);
        assert!(output.contains("Cleanup Summary"));
    }

    // ── Config path independence ───────────────────────────────────────────────

    #[test]
    fn config_paths_are_absolute() {
        let config = Config::default();
        assert!(PathBuf::from(&config.log_file).is_absolute());
        assert!(PathBuf::from(&config.error_log).is_absolute());
        assert!(PathBuf::from(&config.lock_file).is_absolute());
    }
}
