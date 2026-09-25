//! What an import did, per table, plus every warning it raised.

use std::collections::BTreeMap;
use std::fmt;

/// Row outcomes for one table.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TableCounts {
    pub inserted: u64,
    pub updated: u64,
    pub skipped: u64,
}

/// The summary printed after an import; tables are listed alphabetically.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub tables: BTreeMap<&'static str, TableCounts>,
    pub warnings: Vec<String>,
    pub originals_written: u64,
}

impl ImportReport {
    /// The counters for `table`, created at zero on first use.
    pub fn counts(&mut self, table: &'static str) -> &mut TableCounts {
        self.tables.entry(table).or_default()
    }

    /// Records a warning and logs it, so it shows up live and in the summary.
    pub fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        tracing::warn!("{message}");
        self.warnings.push(message);
    }
}

impl fmt::Display for ImportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<20} {:>9} {:>9} {:>9}",
            "table", "inserted", "updated", "skipped"
        )?;
        for (name, c) in &self.tables {
            writeln!(
                f,
                "{name:<20} {:>9} {:>9} {:>9}",
                c.inserted, c.updated, c.skipped
            )?;
        }
        writeln!(f, "originals written: {}", self.originals_written)?;
        if self.warnings.is_empty() {
            return writeln!(f, "warnings: none");
        }
        writeln!(f, "warnings ({}):", self.warnings.len())?;
        self.warnings
            .iter()
            .try_for_each(|w| writeln!(f, "  - {w}"))
    }
}
