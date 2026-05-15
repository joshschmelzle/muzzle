//! Wrappers around `launchctl` invocations.
//!
//! Each function takes a domain target (`system/<label>` or
//! `gui/<uid>/<label>`) and returns a typed result. The domain is rendered
//! here so the rest of the code never assembles launchctl arguments
//! by hand.

use crate::exec::{DEFAULT_TIMEOUT, Output, render_command, run};
use crate::services::{Scope, Service};

/// Fully-qualified launchctl target, e.g. `system/com.zscaler.tunnel`
/// or `gui/501/com.zscaler.tray`.
#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub(crate) domain: String,
    pub(crate) label: String,
}

impl Target {
    pub(crate) fn for_service(svc: &Service, uid: u32) -> Self {
        let domain = match svc.scope {
            Scope::System => "system".to_string(),
            Scope::User => format!("gui/{uid}"),
        };
        Self {
            domain,
            label: svc.label.to_string(),
        }
    }

    /// `<domain>/<label>` — the form launchctl wants for most subcommands.
    pub(crate) fn qualified(&self) -> String {
        format!("{}/{}", self.domain, self.label)
    }
}

/// The verbs muzzle uses against launchctl.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Verb {
    Disable,
    Enable,
    Bootout,
}

impl Verb {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Enable => "enable",
            Self::Bootout => "bootout",
        }
    }
}

/// Build the argv for a verb against a target. Returned as owned strings so
/// the caller can reuse them for rendering and execution.
pub(crate) fn build_argv(verb: Verb, target: &Target) -> Vec<String> {
    vec![verb.as_str().to_string(), target.qualified()]
}

/// Build the argv for `launchctl bootstrap <domain> <plist-path>`. The
/// bootstrap subcommand is shaped differently from `enable` / `disable` /
/// `bootout`: it takes the bare domain plus a plist path, not the
/// `<domain>/<label>` form.
pub(crate) fn build_bootstrap_argv(domain: &str, plist_path: &str) -> Vec<String> {
    vec![
        "bootstrap".to_string(),
        domain.to_string(),
        plist_path.to_string(),
    ]
}

/// Render the launchctl invocation as a string for display.
pub(crate) fn render_argv(argv: &[String]) -> String {
    let borrowed: Vec<&str> = argv.iter().map(String::as_str).collect();
    render_command("launchctl", &borrowed)
}

/// Run `launchctl <verb> <domain>/<label>`.
pub(crate) fn run_verb(verb: Verb, target: &Target) -> std::io::Result<Output> {
    run(
        "launchctl",
        [verb.as_str(), &target.qualified()],
        DEFAULT_TIMEOUT,
    )
}

/// Run `launchctl bootstrap <domain> <plist-path>`.
pub(crate) fn run_bootstrap(domain: &str, plist_path: &str) -> std::io::Result<Output> {
    run(
        "launchctl",
        ["bootstrap", domain, plist_path],
        DEFAULT_TIMEOUT,
    )
}

/// Run `launchctl print-disabled <domain>`.
pub(crate) fn print_disabled(domain: &str) -> std::io::Result<Output> {
    run("launchctl", ["print-disabled", domain], DEFAULT_TIMEOUT)
}

/// Run `launchctl print <domain>/<label>`. Used only for its exit code.
pub(crate) fn print(target: &Target) -> std::io::Result<Output> {
    run("launchctl", ["print", &target.qualified()], DEFAULT_TIMEOUT)
}

/// `bootout` failing because the service isn't loaded is expected. This
/// matches launchctl's error text on macOS:
///
/// > Could not find service "..." in domain ...
/// > Boot-out failed: 113: Could not find specified service
pub(crate) fn is_bootout_not_loaded(o: &Output) -> bool {
    if o.success() {
        return false;
    }
    let blob = format!("{}\n{}", o.stdout, o.stderr).to_lowercase();
    blob.contains("could not find")
        || blob.contains("no such process")
        || blob.contains("not loaded")
        || blob.contains("113")
}

/// `bootstrap` failing because the service is *already* loaded is expected
/// in `muzzle on` if Zscaler somehow stayed up. macOS reports this as
/// "service already loaded" / errno 37 ("Operation already in progress")
/// / errno 17 ("File exists") depending on version.
pub(crate) fn is_bootstrap_already_loaded(o: &Output) -> bool {
    if o.success() {
        return false;
    }
    let blob = format!("{}\n{}", o.stdout, o.stderr).to_lowercase();
    blob.contains("already loaded")
        || blob.contains("already bootstrapped")
        || blob.contains("service already")
        || blob.contains("operation already in progress")
        || blob.contains("37:")
        || blob.contains("17:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{Scope, Service};

    #[test]
    fn target_system() {
        let s = Service {
            label: "com.zscaler.tunnel",
            scope: Scope::System,
        };
        let t = Target::for_service(&s, 501);
        assert_eq!(t.qualified(), "system/com.zscaler.tunnel");
    }

    #[test]
    fn target_user() {
        let s = Service {
            label: "com.zscaler.tray",
            scope: Scope::User,
        };
        let t = Target::for_service(&s, 501);
        assert_eq!(t.qualified(), "gui/501/com.zscaler.tray");
    }

    #[test]
    fn build_argv_disable_system() {
        let s = Service {
            label: "com.zscaler.tunnel",
            scope: Scope::System,
        };
        let t = Target::for_service(&s, 501);
        let argv = build_argv(Verb::Disable, &t);
        assert_eq!(
            argv,
            vec![
                "disable".to_string(),
                "system/com.zscaler.tunnel".to_string()
            ]
        );
    }

    #[test]
    fn render_argv_format() {
        let argv = vec![
            "disable".to_string(),
            "system/com.zscaler.tunnel".to_string(),
        ];
        assert_eq!(
            render_argv(&argv),
            "launchctl disable system/com.zscaler.tunnel"
        );
    }

    #[test]
    fn bootout_not_loaded_detection() {
        let o = Output {
            status: Some(113),
            stdout: String::new(),
            stderr: "Could not find service \"com.zscaler.tunnel\" in domain for system".into(),
            timed_out: false,
        };
        assert!(is_bootout_not_loaded(&o));
    }

    #[test]
    fn bootout_real_failure_not_misclassified() {
        let o = Output {
            status: Some(1),
            stdout: String::new(),
            stderr: "Operation not permitted".into(),
            timed_out: false,
        };
        assert!(!is_bootout_not_loaded(&o));
    }

    #[test]
    fn bootstrap_argv_format() {
        let argv =
            build_bootstrap_argv("system", "/Library/LaunchDaemons/com.zscaler.tunnel.plist");
        assert_eq!(
            argv,
            vec![
                "bootstrap".to_string(),
                "system".to_string(),
                "/Library/LaunchDaemons/com.zscaler.tunnel.plist".to_string(),
            ]
        );
        assert_eq!(
            render_argv(&argv),
            "launchctl bootstrap system /Library/LaunchDaemons/com.zscaler.tunnel.plist"
        );
    }

    #[test]
    fn bootstrap_already_loaded_detection() {
        let o = Output {
            status: Some(37),
            stdout: String::new(),
            stderr: "Bootstrap failed: 37: Operation already in progress".into(),
            timed_out: false,
        };
        assert!(is_bootstrap_already_loaded(&o));
    }

    #[test]
    fn bootstrap_real_failure_not_misclassified() {
        let o = Output {
            status: Some(1),
            stdout: String::new(),
            stderr: "Load failed: no such file".into(),
            timed_out: false,
        };
        assert!(!is_bootstrap_already_loaded(&o));
    }
}
