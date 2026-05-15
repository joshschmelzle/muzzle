//! `muzzle` — toggle Zscaler launchd services on macOS for the current session.
//!
//! Exit codes:
//! * 0 — success
//! * 1 — usage error (handled by clap)
//! * 2 — not running on macOS
//! * 3 — one or more service operations failed
//! * 4 — needed root but couldn't escalate

mod exec;
mod launchctl;
mod output;
mod privilege;
mod services;
mod status;

use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{Level, debug, info, warn};
use tracing_subscriber::EnvFilter;

use crate::launchctl::{
    Target, Verb, build_argv, build_bootstrap_argv, is_bootout_not_loaded,
    is_bootstrap_already_loaded, render_argv, run_bootstrap, run_verb,
};
use crate::output::OpOutcome;
use crate::services::{SERVICES, Scope, Service};
use crate::status::{ServiceStatus, Tri, loaded_from_print_status, parse_disabled};

/// Exit code for "this isn't macOS".
const EX_NOT_MACOS: u8 = 2;
/// Exit code for "at least one launchctl op failed".
const EX_OP_FAILED: u8 = 3;
/// Exit code for "we needed root and couldn't get it".
const EX_NO_ROOT: u8 = 4;

#[derive(Debug, Parser)]
#[command(
    name = "muzzle",
    version,
    about = "Toggle Zscaler launchd services on macOS for the current session."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Disable and bootout all Zscaler launchd services.
    Off {
        /// Print every command that would run without running it.
        #[arg(long)]
        dry_run: bool,
        /// Print each command and its output as it runs.
        #[arg(long, short)]
        verbose: bool,
    },
    /// Re-enable all Zscaler launchd services.
    On {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, short)]
        verbose: bool,
    },
    /// Show current state of each known Zscaler service.
    Status {
        /// Emit machine-readable JSON instead of a table.
        #[arg(long)]
        json: bool,
        #[arg(long, short)]
        verbose: bool,
    },
}

impl Cmd {
    const fn verbose(&self) -> bool {
        match self {
            Self::Off { verbose, .. } | Self::On { verbose, .. } | Self::Status { verbose, .. } => {
                *verbose
            }
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.cmd.verbose());

    if !cfg!(target_os = "macos") {
        let _ = writeln!(io::stderr(), "muzzle only runs on macOS");
        return ExitCode::from(EX_NOT_MACOS);
    }

    match run(&cli.cmd) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(io::stderr(), "muzzle: {e:#}");
            ExitCode::from(EX_OP_FAILED)
        }
    }
}

fn init_tracing(verbose: bool) {
    let default = if verbose { Level::DEBUG } else { Level::WARN };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default.to_string()));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_target(false)
        .without_time()
        .try_init();
}

fn run(cmd: &Cmd) -> Result<ExitCode> {
    match *cmd {
        Cmd::Off { dry_run, verbose } => cmd_off(dry_run, verbose),
        Cmd::On { dry_run, verbose } => cmd_on(dry_run, verbose),
        Cmd::Status { json, verbose: _ } => cmd_status(json),
    }
}

/// Make sure we are root, re-exec'ing under sudo if we aren't. Returns
/// the original (pre-sudo) uid if we are already root. Never returns
/// otherwise: it either `exec`s into a sudo invocation of ourselves or
/// exits with [`EX_NO_ROOT`].
fn ensure_root_for(action: &str) -> u32 {
    if privilege::is_root() {
        return privilege::original_uid();
    }
    eprintln!(
        "muzzle needs root for system-level Zscaler services ({action}); re-running under sudo..."
    );
    // reexec_with_sudo never returns on success; if it does return, it's Err.
    match privilege::reexec_with_sudo() {
        Ok(_never) => unreachable!("reexec_with_sudo returned Ok"),
        Err(e) => {
            let _ = writeln!(io::stderr(), "muzzle: could not escalate via sudo: {e:#}");
            std::process::exit(i32::from(EX_NO_ROOT));
        }
    }
}

fn cmd_off(dry_run: bool, verbose: bool) -> Result<ExitCode> {
    // In dry-run mode we never touch launchctl, so we don't need root
    // either; just use the original uid we can detect.
    let uid = if dry_run || !services::has_system_services() {
        privilege::original_uid()
    } else {
        ensure_root_for("off")
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    output::header_off(&mut out)?;

    let mut any_failed = false;
    for svc in SERVICES {
        let target = Target::for_service(svc, uid);
        let outcome = do_off_one(svc, &target, dry_run, verbose, &mut out)?;
        if !outcome.ok {
            any_failed = true;
        }
        output::line(&mut out, &outcome)?;
    }

    output::footer_off(&mut out)?;
    Ok(if any_failed {
        ExitCode::from(EX_OP_FAILED)
    } else {
        ExitCode::SUCCESS
    })
}

fn do_off_one(
    svc: &Service,
    target: &Target,
    dry_run: bool,
    verbose: bool,
    out: &mut impl Write,
) -> Result<OpOutcome> {
    // 1. disable — failure here is a real error.
    let disable_argv = build_argv(Verb::Disable, target);
    if dry_run {
        output::dry_run_line(out, &render_argv(&disable_argv))?;
    } else {
        if verbose {
            output::dry_run_line(out, &render_argv(&disable_argv))?;
        }
        let o = run_verb(Verb::Disable, target)?;
        log_output(verbose, out, &o)?;
        if !o.success() {
            let reason =
                first_error_line(&o.stderr).unwrap_or_else(|| format!("exit {:?}", o.status));
            return Ok(OpOutcome {
                service: *svc,
                ok: false,
                reason: Some(format!("disable: {reason}")),
            });
        }
    }

    // 2. bootout — "not loaded" is fine.
    let bootout_argv = build_argv(Verb::Bootout, target);
    if dry_run {
        output::dry_run_line(out, &render_argv(&bootout_argv))?;
    } else {
        if verbose {
            output::dry_run_line(out, &render_argv(&bootout_argv))?;
        }
        let o = run_verb(Verb::Bootout, target)?;
        log_output(verbose, out, &o)?;
        if !o.success() && !is_bootout_not_loaded(&o) {
            let reason =
                first_error_line(&o.stderr).unwrap_or_else(|| format!("exit {:?}", o.status));
            return Ok(OpOutcome {
                service: *svc,
                ok: false,
                reason: Some(format!("bootout: {reason}")),
            });
        } else if !o.success() {
            debug!(
                label = svc.label,
                "bootout reported service not loaded (ok)"
            );
        }
    }

    Ok(OpOutcome {
        service: *svc,
        ok: true,
        reason: None,
    })
}

fn cmd_on(dry_run: bool, verbose: bool) -> Result<ExitCode> {
    let uid = if dry_run || !services::has_system_services() {
        privilege::original_uid()
    } else {
        ensure_root_for("on")
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    output::header_on(&mut out)?;

    let mut any_failed = false;
    for svc in SERVICES {
        let target = Target::for_service(svc, uid);
        let outcome = do_on_one(svc, &target, dry_run, verbose, &mut out)?;
        if !outcome.ok {
            any_failed = true;
        }
        output::line(&mut out, &outcome)?;
    }

    output::footer_on(&mut out)?;
    Ok(if any_failed {
        ExitCode::from(EX_OP_FAILED)
    } else {
        ExitCode::SUCCESS
    })
}

fn do_on_one(
    svc: &Service,
    target: &Target,
    dry_run: bool,
    verbose: bool,
    out: &mut impl Write,
) -> Result<OpOutcome> {
    // 1. enable — failure here is a real error.
    let enable_argv = build_argv(Verb::Enable, target);
    if dry_run {
        output::dry_run_line(out, &render_argv(&enable_argv))?;
    } else {
        if verbose {
            output::dry_run_line(out, &render_argv(&enable_argv))?;
        }
        let o = run_verb(Verb::Enable, target)?;
        log_output(verbose, out, &o)?;
        if !o.success() {
            let reason =
                first_error_line(&o.stderr).unwrap_or_else(|| format!("exit {:?}", o.status));
            return Ok(OpOutcome {
                service: *svc,
                ok: false,
                reason: Some(format!("enable: {reason}")),
            });
        }
    }

    // 2. bootstrap — bring the service back up in the current session.
    //    "already loaded" is fine; anything else is a real error.
    let plist = svc.plist_path();
    let bootstrap_argv = build_bootstrap_argv(&target.domain, &plist);
    if dry_run {
        output::dry_run_line(out, &render_argv(&bootstrap_argv))?;
    } else {
        if verbose {
            output::dry_run_line(out, &render_argv(&bootstrap_argv))?;
        }
        let o = run_bootstrap(&target.domain, &plist)?;
        log_output(verbose, out, &o)?;
        if !o.success() && !is_bootstrap_already_loaded(&o) {
            let reason =
                first_error_line(&o.stderr).unwrap_or_else(|| format!("exit {:?}", o.status));
            return Ok(OpOutcome {
                service: *svc,
                ok: false,
                reason: Some(format!("bootstrap: {reason}")),
            });
        } else if !o.success() {
            debug!(
                label = svc.label,
                "bootstrap reported service already loaded (ok)"
            );
        }
    }

    Ok(OpOutcome {
        service: *svc,
        ok: true,
        reason: None,
    })
}

fn cmd_status(json: bool) -> Result<ExitCode> {
    // status is best-effort: we don't auto-escalate, but we do try the
    // system print-disabled call without sudo and let the user see what
    // they get. If they need a real picture, they can `sudo muzzle status`.
    let uid = privilege::original_uid();

    // Cache print-disabled output per domain.
    let system_disabled = match launchctl::print_disabled("system") {
        Ok(o) => o.stdout,
        Err(e) => {
            warn!("could not read system print-disabled: {e}");
            String::new()
        }
    };
    let gui_domain = format!("gui/{uid}");
    let gui_disabled = match launchctl::print_disabled(&gui_domain) {
        Ok(o) => o.stdout,
        Err(e) => {
            warn!("could not read {gui_domain} print-disabled: {e}");
            String::new()
        }
    };

    let mut rows = Vec::with_capacity(SERVICES.len());
    for svc in SERVICES {
        let target = Target::for_service(svc, uid);
        let disabled = match svc.scope {
            Scope::System => parse_disabled(&system_disabled, svc.label),
            Scope::User => parse_disabled(&gui_disabled, svc.label),
        };
        let loaded = match launchctl::print(&target) {
            Ok(o) => loaded_from_print_status(o.status),
            Err(_) => Tri::Unknown,
        };
        rows.push(ServiceStatus {
            label: svc.label.to_string(),
            scope: svc.scope,
            disabled,
            loaded,
        });
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        output::status_json(&mut out, &rows)?;
    } else {
        output::status_table(&mut out, &rows)?;
    }

    info!("status checked {} services", rows.len());
    Ok(ExitCode::SUCCESS)
}

fn log_output(verbose: bool, out: &mut impl Write, o: &exec::Output) -> io::Result<()> {
    if !verbose {
        return Ok(());
    }
    if !o.stdout.is_empty() {
        for line in o.stdout.lines() {
            writeln!(out, "    │ {line}")?;
        }
    }
    if !o.stderr.is_empty() {
        for line in o.stderr.lines() {
            writeln!(out, "    │ {line}")?;
        }
    }
    Ok(())
}

fn first_error_line(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}
