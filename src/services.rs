//! Hardcoded list of Zscaler launchd services that `muzzle` manages.
//!
//! Adding a new label is a one-line change to [`SERVICES`]. There is no
//! auto-discovery; the explicit list is the contract.

use serde::Serialize;

/// Whether a launchd service lives in the system domain (root) or the
/// per-user GUI domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scope {
    /// `system/<label>` — requires root.
    System,
    /// `gui/<uid>/<label>` — runs in the user's Aqua session.
    User,
}

impl Scope {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }

    /// Directory under which a service of this scope's plist lives.
    pub(crate) const fn plist_dir(self) -> &'static str {
        match self {
            Self::System => "/Library/LaunchDaemons",
            Self::User => "/Library/LaunchAgents",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A Zscaler launchd service we know how to toggle.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Service {
    pub(crate) label: &'static str,
    pub(crate) scope: Scope,
}

impl Service {
    /// Conventional plist path for this service:
    /// `/Library/LaunchDaemons/<label>.plist` for system services,
    /// `/Library/LaunchAgents/<label>.plist` for user services.
    pub(crate) fn plist_path(&self) -> String {
        format!("{}/{}.plist", self.scope.plist_dir(), self.label)
    }
}

/// The full set of services `muzzle` operates on.
///
/// Adding a new label is a one-line change.
///
/// `com.zscaler.preloginui` is intentionally absent: it has
/// `LimitLoadToSessionType = LoginWindow` and lives in `loginwindow/<uid>`,
/// a domain macOS Tahoe refuses to read or write from a user-session
/// shell even under `sudo`. It also doesn't run during a user session,
/// so it's outside muzzle's stated scope. See the README for details.
pub(crate) const SERVICES: &[Service] = &[
    Service {
        label: "com.zscaler.tunnel",
        scope: Scope::System,
    },
    Service {
        label: "com.zscaler.service",
        scope: Scope::System,
    },
    Service {
        label: "com.zscaler.UPMServiceController",
        scope: Scope::System,
    },
    Service {
        label: "com.zscaler.tray",
        scope: Scope::User,
    },
];

/// Returns true if any system-scope service is present.
pub(crate) fn has_system_services() -> bool {
    SERVICES.iter().any(|s| s.scope == Scope::System)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn services_non_empty() {
        assert!(!SERVICES.is_empty());
    }

    #[test]
    fn labels_are_unique() {
        let mut seen = HashSet::new();
        for s in SERVICES {
            assert!(seen.insert(s.label), "duplicate label: {}", s.label);
        }
    }

    #[test]
    fn has_system_services_true() {
        assert!(has_system_services());
    }

    #[test]
    fn preloginui_not_managed() {
        assert!(
            !SERVICES.iter().any(|s| s.label == "com.zscaler.preloginui"),
            "preloginui must not be in SERVICES; see module docs"
        );
    }
}
