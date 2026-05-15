//! Integration tests that exercise the CLI without ever touching launchctl
//! against the real system. We rely entirely on `--dry-run` and on the
//! `--help` / `--version` paths.

#![allow(
    clippy::expect_used,
    reason = "test code is allowed to assert via expect"
)]

use assert_cmd::Command;
use predicates::str::contains;

fn muzzle() -> Command {
    Command::cargo_bin("muzzle").expect("muzzle binary")
}

#[test]
fn version_works() {
    muzzle().arg("--version").assert().success();
}

#[test]
fn help_works() {
    muzzle()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Toggle Zscaler"));
}

#[test]
fn status_help_works() {
    muzzle().args(["status", "--help"]).assert().success();
}

#[test]
fn on_dry_run_prints_each_command() {
    let assert = muzzle().args(["on", "--dry-run"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8");

    let enables = stdout.matches("launchctl enable ").count();
    let bootstraps = stdout.matches("launchctl bootstrap ").count();
    assert_eq!(
        enables, 4,
        "expected 4 enable lines, got {enables}: {stdout}"
    );
    assert_eq!(
        bootstraps, 4,
        "expected 4 bootstrap lines, got {bootstraps}: {stdout}"
    );

    // System services should bootstrap from /Library/LaunchDaemons,
    // user services from /Library/LaunchAgents.
    assert!(
        stdout
            .contains("launchctl bootstrap system /Library/LaunchDaemons/com.zscaler.tunnel.plist"),
        "{stdout}"
    );
    assert!(
        stdout.contains("/Library/LaunchAgents/com.zscaler.tray.plist"),
        "{stdout}"
    );

    // preloginui is intentionally unmanaged.
    assert!(
        !stdout.contains("preloginui"),
        "preloginui must not appear: {stdout}"
    );
    assert!(
        !stdout.contains("loginwindow"),
        "loginwindow scope must not appear: {stdout}"
    );

    // No disable / bootout in `on`.
    assert!(
        !stdout.contains("launchctl disable "),
        "unexpected disable: {stdout}"
    );
    assert!(
        !stdout.contains("launchctl bootout "),
        "unexpected bootout: {stdout}"
    );
}

#[test]
fn off_dry_run_prints_each_command() {
    let assert = muzzle().args(["off", "--dry-run"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8");

    // System-scope services should appear as system/<label>.
    assert!(
        stdout.contains("launchctl disable system/com.zscaler.tunnel"),
        "{stdout}"
    );
    assert!(
        stdout.contains("launchctl bootout system/com.zscaler.tunnel"),
        "{stdout}"
    );
    assert!(
        stdout.contains("launchctl disable system/com.zscaler.service"),
        "{stdout}"
    );
    assert!(
        stdout.contains("launchctl disable system/com.zscaler.UPMServiceController"),
        "{stdout}"
    );

    // User-scope services should appear as gui/<uid>/<label>.
    assert!(stdout.contains("/com.zscaler.tray"), "{stdout}");
    // preloginui is intentionally unmanaged.
    assert!(
        !stdout.contains("preloginui"),
        "preloginui must not appear: {stdout}"
    );
    assert!(
        !stdout.contains("loginwindow"),
        "loginwindow scope must not appear: {stdout}"
    );

    // Each managed service has two lines: disable + bootout.
    let disables = stdout.matches("launchctl disable ").count();
    let bootouts = stdout.matches("launchctl bootout ").count();
    assert_eq!(
        disables, 4,
        "expected 4 disable lines, got {disables}: {stdout}"
    );
    assert_eq!(
        bootouts, 4,
        "expected 4 bootout lines, got {bootouts}: {stdout}"
    );

    assert!(stdout.contains("Done."), "missing footer: {stdout}");
}

#[test]
fn unknown_subcommand_is_usage_error() {
    muzzle().arg("frobnicate").assert().failure().code(2);
}
