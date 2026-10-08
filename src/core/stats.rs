use std::fmt;

#[derive(Debug, Default)]
pub struct UpdateStats {
    pub formulae: ComponentStats,
    pub casks: CaskStats,
    pub bun: ComponentStats,
}

#[derive(Debug, Default)]
pub struct ComponentStats {
    pub upgraded: usize,
    pub skipped: usize,
}

#[derive(Debug, Default)]
pub struct CaskStats {
    pub upgraded: usize,
    pub recovered: usize,
    pub skipped_manual: Vec<String>,
    pub skipped_running: Vec<String>,
    pub skipped_auth: Vec<String>,
    pub skipped_invalid: Vec<String>,
    pub skipped_source_missing: Vec<String>,
    pub skipped_timeout: Vec<String>,
    pub skipped_other: Vec<String>,
}

impl CaskStats {
    pub fn total_skipped(&self) -> usize {
        self.skipped_manual.len()
            + self.skipped_running.len()
            + self.skipped_auth.len()
            + self.skipped_invalid.len()
            + self.skipped_source_missing.len()
            + self.skipped_timeout.len()
            + self.skipped_other.len()
    }

    pub fn has_actionable_items(&self) -> bool {
        !self.skipped_running.is_empty() || !self.skipped_source_missing.is_empty()
    }
}

impl fmt::Display for UpdateStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n=== Update Summary ===")?;

        // Formulae
        writeln!(f, "Formulae: {} upgraded", self.formulae.upgraded)?;
        if self.formulae.skipped > 0 {
            writeln!(f, "         {} skipped", self.formulae.skipped)?;
        }

        // Casks
        let cask_summary = if self.casks.recovered > 0 {
            format!(
                "Casks:    {} upgraded, {} recovered, {} skipped",
                self.casks.upgraded,
                self.casks.recovered,
                self.casks.total_skipped()
            )
        } else {
            format!(
                "Casks:    {} upgraded, {} skipped",
                self.casks.upgraded,
                self.casks.total_skipped()
            )
        };
        writeln!(f, "{}", cask_summary)?;

        if !self.casks.skipped_running.is_empty() {
            writeln!(
                f,
                "  ⚠ Running apps ({}):",
                self.casks.skipped_running.len()
            )?;
            for app in &self.casks.skipped_running {
                writeln!(f, "    - {}", app)?;
            }
            writeln!(f, "    → Close these apps and run again")?;
        }

        if !self.casks.skipped_manual.is_empty() {
            writeln!(
                f,
                "  ℹ Manual installers ({}):",
                self.casks.skipped_manual.len()
            )?;
            for app in &self.casks.skipped_manual {
                writeln!(f, "    - {}", app)?;
            }
            writeln!(f, "    → Update these apps manually")?;
        }

        if !self.casks.skipped_auth.is_empty() {
            writeln!(
                f,
                "  🔒 Requires authentication ({}):",
                self.casks.skipped_auth.len()
            )?;
            for app in &self.casks.skipped_auth {
                writeln!(f, "    - {}", app)?;
            }
            writeln!(f, "    → Update manually or configure authentication")?;
        }

        if !self.casks.skipped_source_missing.is_empty() {
            writeln!(
                f,
                "  📁 App source issues ({}):",
                self.casks.skipped_source_missing.len()
            )?;
            for app in &self.casks.skipped_source_missing {
                writeln!(f, "    - {}", app)?;
            }
            writeln!(f, "    → Apps may be in non-standard locations")?;
        }

        if !self.casks.skipped_timeout.is_empty() {
            writeln!(f, "  ⏱ Timed out ({}):", self.casks.skipped_timeout.len())?;
            for app in &self.casks.skipped_timeout {
                writeln!(f, "    - {}", app)?;
            }
            writeln!(f, "    → Large downloads or slow network; try again later")?;
        }

        if !self.casks.skipped_invalid.is_empty() {
            writeln!(
                f,
                "  ❌ Invalid definitions ({}):",
                self.casks.skipped_invalid.len()
            )?;
            for app in &self.casks.skipped_invalid {
                writeln!(f, "    - {}", app)?;
            }
            writeln!(f, "    → Check stout-index or run stout update")?;
        }

        if !self.casks.skipped_other.is_empty() {
            writeln!(f, "  ⚠ Other issues ({}):", self.casks.skipped_other.len())?;
            for app in &self.casks.skipped_other {
                writeln!(f, "    - {}", app)?;
            }
        }

        // Bun
        if self.bun.upgraded > 0 || self.bun.skipped > 0 {
            writeln!(f, "Bun:      {} upgraded", self.bun.upgraded)?;
            if self.bun.skipped > 0 {
                writeln!(f, "         {} with warnings", self.bun.skipped)?;
            }
        }

        writeln!(f, "======================")?;

        Ok(())
    }
}
