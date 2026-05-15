# muzzle

Toggle services's launchd-managed processes on macOS for the current session.

## Why this exists

Certain services run as a combination of system-level launchd daemons (as root) and
per-user launchd agents (in your GUI session). When you need traffic to stop
being intercepted — debugging a TLS issue, talking to a host the service
inspection chokes on, running a packet capture you can actually read —
flipping all of them off and back on with `launchctl` by hand is tedious and
easy to get wrong.

`muzzle` is a one-shot CLI that does the toggling correctly. It is not a
daemon. It runs, makes its changes, prints what it did, and exits.

## Install

```sh
cargo install --path .
```

## Usage

```sh
muzzle off          # disable and bootout all launchd services
muzzle on           # re-enable all launchd services
muzzle status       # show current state of each known service
muzzle status --json
muzzle off --dry-run
muzzle on  --dry-run --verbose
muzzle --version
```

### Example

```text
$ muzzle off
Done. Run `muzzle on` to re-enable.
```

## Things to know

* **Re-exec to root.** `muzzle off` and `muzzle on` need root for the
  system-level services. If you're not already root, `muzzle` re-execs
  itself under `sudo`, preserving your arguments. It uses `SUDO_UID` /
  `MUZZLE_ORIG_UID` to remember the original user so user-scope launchctl
  calls still target the right `gui/<uid>` domain.
* **`--dry-run` is your friend.** It prints every command that would be
  executed without running it, never touches launchctl, and does not even
  trigger the sudo re-exec. Use it whenever you're unsure.
* **`muzzle on` brings the services back up in the current session.** For
  each service it runs `launchctl enable` and then `launchctl bootstrap
  <domain> <plist>` against the conventional plist path
  (`/Library/LaunchDaemons/<label>.plist` for system services,
  `/Library/LaunchAgents/<label>.plist` for user services). `bootstrap`
  failing because the service is already loaded is treated as success.
* **`bootout` failing because a service isn't loaded is expected** during
  `off` and is not treated as an error. `disable` / `enable` failing is.

## Services

The set of services is hardcoded in `src/services.rs`:

To add another label, append one entry to the `SERVICES` array and rebuild.
There is no auto-discovery; the explicit list is the contract.

## Exit codes

| Code | Meaning                                            |
| ---- | -------------------------------------------------- |
| 0    | All operations succeeded                           |
| 1    | Usage error (handled by clap)                      |
| 2    | Not running on macOS                               |
| 3    | One or more service operations failed              |
| 4    | Needed root, couldn't escalate via sudo            |

## Development

```sh
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo test
```

Tests are fixture-based and never invoke `launchctl` against the live
system. Captured outputs of `launchctl print-disabled` live under
`tests/fixtures/`.

## Disclaimer

It only toggles launchd state. It does not install, uninstall, or modify
the service itself.
