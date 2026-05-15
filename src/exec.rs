//! Thin wrapper around [`std::process::Command`] that captures stdout/stderr
//! and enforces a per-command timeout via spawn + poll. No tokio.

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Result of running a subprocess.
#[derive(Debug, Clone)]
pub(crate) struct Output {
    pub(crate) status: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
}

impl Output {
    pub(crate) fn success(&self) -> bool {
        !self.timed_out && self.status == Some(0)
    }
}

/// Default per-command timeout.
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Spawn a command, wait up to `timeout` for it to exit. If it doesn't,
/// kill it and report `timed_out = true`.
pub(crate) fn run<I, S>(program: &str, args: I, timeout: Duration) -> std::io::Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let start = Instant::now();
    let poll = Duration::from_millis(25);

    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut o) = child.stdout.take() {
                let _ = o.read_to_string(&mut stdout);
            }
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut stderr);
            }
            return Ok(Output {
                status: None,
                stdout,
                stderr,
                timed_out: true,
            });
        }
        std::thread::sleep(poll);
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }

    Ok(Output {
        status: status.code(),
        stdout,
        stderr,
        timed_out: false,
    })
}

/// Render a command line for display (dry-run, --verbose). Arguments are
/// joined with spaces; arguments containing whitespace are surrounded with
/// single quotes. This is purely for display — we never pass through a shell.
pub(crate) fn render_command(program: &str, args: &[&str]) -> String {
    let mut out = String::from(program);
    for a in args {
        out.push(' ');
        if a.is_empty() || a.chars().any(char::is_whitespace) {
            out.push('\'');
            out.push_str(a);
            out.push('\'');
        } else {
            out.push_str(a);
        }
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test code is allowed to assert via panic"
)]
mod tests {
    use super::*;

    #[test]
    fn render_basic() {
        assert_eq!(
            render_command("launchctl", &["disable", "system/com.zscaler.tunnel"]),
            "launchctl disable system/com.zscaler.tunnel"
        );
    }

    #[test]
    fn render_quotes_whitespace() {
        assert_eq!(
            render_command("echo", &["hello world"]),
            "echo 'hello world'"
        );
    }

    #[test]
    fn run_true_succeeds() {
        let Ok(o) = run("true", std::iter::empty::<&str>(), DEFAULT_TIMEOUT) else {
            panic!("spawn true");
        };
        assert!(o.success());
    }

    #[test]
    fn run_false_fails() {
        let Ok(o) = run("false", std::iter::empty::<&str>(), DEFAULT_TIMEOUT) else {
            panic!("spawn false");
        };
        assert!(!o.success());
        assert_eq!(o.status, Some(1));
    }
}
