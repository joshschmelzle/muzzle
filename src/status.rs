//! Parsers for `launchctl print-disabled <domain>` and helpers that turn
//! a service + domain into a [`ServiceStatus`] row.
//!
//! The `print-disabled` output looks roughly like:
//!
//! ```text
//! Disabled services for system:
//!     "com.apple.something" => disabled
//!     "com.zscaler.tunnel" => enabled
//! ```
//!
//! Older / newer macOS variants emit slightly different forms — `=> true` /
//! `=> false`, unquoted labels, indentation differences. The parser is
//! lenient: it scans for the label as a whole token on a line and reads the
//! state token that follows `=>`.

use serde::Serialize;

use crate::services::Scope;

/// What we know about a single service's launchd state.
#[derive(Debug, Clone, Serialize, tabled::Tabled)]
pub(crate) struct ServiceStatus {
    pub(crate) label: String,
    pub(crate) scope: Scope,
    #[tabled(display = "display_tri")]
    pub(crate) disabled: Tri,
    #[tabled(display = "display_tri")]
    pub(crate) loaded: Tri,
}

/// Tri-state for "we couldn't tell" cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Tri {
    True,
    False,
    Unknown,
}

impl Tri {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Unknown => "unknown",
        }
    }
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "tabled display fns take &T"
)]
fn display_tri(t: &Tri) -> String {
    t.as_str().to_string()
}

/// Find the disabled state for `label` inside the output of
/// `launchctl print-disabled <domain>`.
///
/// Returns:
/// - `Tri::True` if the label is listed as disabled,
/// - `Tri::False` if it is listed and enabled,
/// - `Tri::Unknown` if the label does not appear in the output.
pub(crate) fn parse_disabled(output: &str, label: &str) -> Tri {
    for line in output.lines() {
        let Some(arrow) = line.find("=>") else {
            continue;
        };
        let key = line.get(..arrow).unwrap_or("").trim().trim_matches('"');
        if key != label {
            continue;
        }
        let value = line
            .get(arrow + 2..)
            .unwrap_or("")
            .trim()
            .trim_end_matches(';')
            .trim()
            .trim_matches('"')
            .to_ascii_lowercase();
        return match value.as_str() {
            "true" | "disabled" => Tri::True,
            "false" | "enabled" => Tri::False,
            _ => Tri::Unknown,
        };
    }
    Tri::Unknown
}

/// Interpret the exit status of `launchctl print <target>` as a loaded /
/// not-loaded answer.
pub(crate) const fn loaded_from_print_status(status: Option<i32>) -> Tri {
    match status {
        Some(0) => Tri::True,
        Some(_) => Tri::False,
        None => Tri::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYS_FIXTURE: &str = include_str!("../tests/fixtures/print-disabled-system.txt");
    const GUI_FIXTURE: &str = include_str!("../tests/fixtures/print-disabled-gui.txt");

    #[test]
    fn parses_disabled_true_from_system_fixture() {
        assert_eq!(parse_disabled(SYS_FIXTURE, "com.zscaler.tunnel"), Tri::True);
    }

    #[test]
    fn parses_disabled_false_from_system_fixture() {
        assert_eq!(
            parse_disabled(SYS_FIXTURE, "com.zscaler.service"),
            Tri::False
        );
    }

    #[test]
    fn parses_missing_label_as_unknown() {
        assert_eq!(
            parse_disabled(SYS_FIXTURE, "com.example.does.not.exist"),
            Tri::Unknown
        );
    }

    #[test]
    fn parses_disabled_true_from_gui_fixture() {
        assert_eq!(parse_disabled(GUI_FIXTURE, "com.zscaler.tray"), Tri::True);
    }

    #[test]
    fn parses_disabled_false_from_gui_fixture() {
        assert_eq!(
            parse_disabled(GUI_FIXTURE, "com.zscaler.preloginui"),
            Tri::False
        );
    }

    #[test]
    fn loaded_status_mapping() {
        assert_eq!(loaded_from_print_status(Some(0)), Tri::True);
        assert_eq!(loaded_from_print_status(Some(113)), Tri::False);
        assert_eq!(loaded_from_print_status(None), Tri::Unknown);
    }
}
