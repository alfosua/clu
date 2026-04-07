//! Tiny unified-diff helper shared by `write` and `edit` tools.

use similar::{ChangeTag, TextDiff};

/// Render a unified diff between `old` and `new`. Returns an empty string when
/// the inputs are identical.
pub fn unified(label: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    out.push_str(&format!("--- {label} (before)\n+++ {label} (after)\n"));
    for group in diff.grouped_ops(3) {
        for op in group {
            for change in diff.iter_changes(&op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                out.push_str(sign);
                out.push_str(change.value());
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}
