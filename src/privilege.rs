//! Privilege handling: detect whether we're root, and re-exec via `sudo` if
//! we aren't. We also stash the pre-escalation UID so user-scope launchctl
//! calls can be re-targeted at the original GUI session.

use std::env;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Environment variable we use to remember the original (pre-sudo) uid so
/// the re-execed process still knows who the user is.
const MUZZLE_ORIG_UID: &str = "MUZZLE_ORIG_UID";

/// Returns the effective uid of the current process.
pub(crate) fn geteuid() -> u32 {
    // Safety justification: `geteuid(2)` is documented as always succeeding
    // and is async-signal-safe. We can't avoid one libc call here without
    // pulling in the `nix` crate, which the spec discourages.
    #[allow(unsafe_code, reason = "single libc call; geteuid cannot fail")]
    unsafe {
        libc_geteuid()
    }
}

// Hand-rolled extern to avoid pulling `libc` in as a direct dependency.
// `geteuid` is part of the platform ABI on macOS.
#[allow(unsafe_code, reason = "extern declaration for libc geteuid")]
unsafe extern "C" {
    #[link_name = "geteuid"]
    fn libc_geteuid() -> u32;
}

/// True if we are currently root.
pub(crate) fn is_root() -> bool {
    geteuid() == 0
}

/// Find the uid of the human user who invoked us, even if we are now root.
///
/// Priority:
/// 1. `MUZZLE_ORIG_UID` if we set it on re-exec.
/// 2. `SUDO_UID` if sudo set it.
/// 3. Our own euid if neither is present.
pub(crate) fn original_uid() -> u32 {
    if let Some(v) = env::var_os(MUZZLE_ORIG_UID).as_deref().and_then(parse_uid) {
        return v;
    }
    if let Some(v) = env::var_os("SUDO_UID").as_deref().and_then(parse_uid) {
        return v;
    }
    geteuid()
}

fn parse_uid(s: &std::ffi::OsStr) -> Option<u32> {
    s.to_str().and_then(|s| s.parse().ok())
}

/// Re-exec the current binary under `sudo`, preserving the original
/// arguments. Sets `MUZZLE_ORIG_UID` so the re-execed copy knows who we
/// were. Replaces this process via `execvp`; only returns on error.
pub(crate) fn reexec_with_sudo() -> Result<std::convert::Infallible> {
    let exe = env::current_exe().context("locating current executable")?;
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let uid = geteuid();

    let mut cmd = Command::new("sudo");
    cmd.arg("--preserve-env=MUZZLE_ORIG_UID,NO_COLOR,RUST_LOG")
        .env(MUZZLE_ORIG_UID, uid.to_string())
        .arg("--")
        .arg(&exe)
        .args(&args);

    let err = cmd.exec(); // never returns on success
    bail!("failed to re-exec under sudo: {err}")
}
