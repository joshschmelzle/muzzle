//! Human-readable and JSON renderers for the various command outputs.

use std::io::{self, Write};

use owo_colors::{OwoColorize, Stream::Stdout};
use tabled::{Table, settings::Style};

use crate::services::Service;
use crate::status::ServiceStatus;

/// Outcome of a single per-service operation in `off` / `on`.
#[derive(Debug, Clone)]
pub(crate) struct OpOutcome {
    pub(crate) service: Service,
    pub(crate) ok: bool,
    pub(crate) reason: Option<String>,
}

/// Header line for `muzzle off`.
pub(crate) fn header_off(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "Disabling Zscaler for this session.")
}

/// Header line for `muzzle on`.
pub(crate) fn header_on(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "Re-enabling Zscaler launchd services.")
}

/// Width to which we pad service labels in the terse summary, so the
/// scope column lines up. Picked to fit the longest current label.
const LABEL_PAD: usize = 32;

/// Render one summary line: `  ✓ com.zscaler.tunnel             (system)`.
pub(crate) fn line(out: &mut impl Write, outcome: &OpOutcome) -> io::Result<()> {
    let mark = if outcome.ok {
        "✓"
            .if_supports_color(Stdout, OwoColorize::green)
            .to_string()
    } else {
        "✗".if_supports_color(Stdout, OwoColorize::red).to_string()
    };
    let scope = outcome.service.scope.as_str();
    if outcome.ok {
        writeln!(
            out,
            "  {mark} {label:<width$} ({scope})",
            label = outcome.service.label,
            width = LABEL_PAD,
        )
    } else {
        let reason = outcome.reason.as_deref().unwrap_or("unknown error");
        writeln!(
            out,
            "  {mark} {label:<width$} ({scope}) failed: {reason}",
            label = outcome.service.label,
            width = LABEL_PAD,
        )
    }
}

/// Trailing block for `muzzle off`.
pub(crate) fn footer_off(out: &mut impl Write) -> io::Result<()> {
    writeln!(
        out,
        "Done. Run `muzzle on` to re-enable."
    )
}

/// Trailing block for `muzzle on`.
pub(crate) fn footer_on(out: &mut impl Write) -> io::Result<()> {
    writeln!(
        out,
        "Done. Zscaler launchd services are enabled and bootstrapped in the current session."
    )
}

/// Render the status table to stdout.
pub(crate) fn status_table(out: &mut impl Write, rows: &[ServiceStatus]) -> io::Result<()> {
    let mut table = Table::new(rows);
    table.with(Style::rounded());
    writeln!(out, "{table}")
}

/// Render the status table as JSON.
pub(crate) fn status_json(out: &mut impl Write, rows: &[ServiceStatus]) -> io::Result<()> {
    let s = serde_json::to_string_pretty(rows).map_err(io::Error::other)?;
    writeln!(out, "{s}")
}

/// Print a dry-run command line: `+ launchctl disable system/...`.
pub(crate) fn dry_run_line(out: &mut impl Write, rendered: &str) -> io::Result<()> {
    let plus = "+"
        .if_supports_color(Stdout, OwoColorize::yellow)
        .to_string();
    writeln!(out, "{plus} {rendered}")
}
