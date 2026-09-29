# Helper supervision (development)

These user units are under development and are not installed by the QML plugin.
The UI connects through a bridge and displays connection state, rooms and recent
history snapshots, and scoped message delivery outcomes.

The socket activates `%h/.local/bin/omarchy-buzz daemon`. It retains its private
runtime directory while the daemon exits after its last client disconnects.
Normal idle exits do not restart the daemon. Failed processes restart at most
three times per minute. Memory, task, descriptor, and shutdown limits constrain
helper failures; they do not make the shared Omarchy QML process a sandbox.

Build and test the helper, then package the trusted local build from the same
checkout as the plugin. The package command verifies its version, pinned Buzz
revision, and architecture. Review third-party notices before distribution.

```bash
python3 scripts/package-helper helper/target/release/omarchy-buzz /tmp/buzz-helper-package
python3 scripts/helper-install install --dry-run /tmp/buzz-helper-package/omarchy-buzz-*-linux-*.tar.gz
python3 scripts/helper-install install /tmp/buzz-helper-package/omarchy-buzz-*-linux-*.tar.gz
```

Use an empty package output directory and one matching archive at a time. The
installer reads the adjacent `.sha256.json` sidecar, validates package members,
binary hash, architecture and reviewed unit templates, then installs the binary
and both user units. A sidecar detects accidental corruption; it does not prove
who created the package. Only install an artifact you built or obtained through
a trusted, independently verified channel. No download is performed.
`--dry-run` checks package structure and previews installed paths without running
the new binary. Actual installation first runs its bounded `--version` command
from a private temporary directory and checks the reported version and Buzz pin;
this checks target loadability before stopping the old service.

The same `install` command upgrades an existing complete installation. It
rejects symlinks, partial installations, edited units, unexpected directories,
and architecture mismatches. It stops the old helper before replacement and
keeps copies of all three previous files under
`~/.local/share/omarchy-buzz/backups/`. The tool reloads systemd and enables
the socket. If activation fails, it restores the old files and prior socket
state. Review an error before retrying; no configuration or identity is changed.

To remove only the helper and owned user units:

```bash
python3 scripts/helper-install uninstall --dry-run
python3 scripts/helper-install uninstall
```

No relay connection is attempted without an explicitly enrolled identity.
Enrollment requires the user's Secret Service, accessed through session D-Bus.
The service deliberately does not accept credential environment files. Do not
enable upstream payload logging in a unit override.

After changing helper configuration, select Retry in the panel. A service restart
is an alternative when no UI is connected.

Disable/remove the QML plugin first. Uninstall preserves a rollback copy of the
three removed files and leaves configuration, Secret Service identity, and the
delivery ledger in place. Omarchy's native plugin removal cannot remove these
separately installed files. The installer never uses root privileges or changes
system units.

Validation so far: installed systemd socket activation passed with unconfigured
public status, and an isolated inherited-socket test passed idle exit/reactivation.
Secret Service enrollment/retrieval passed in a separate private test session.
Keyring access under every service hardening setting and resource limits under
hostile relay load still require validation.

The sender owns a private durable ledger under
`$XDG_STATE_HOME/omarchy-buzz/delivery/ledger.json` (default
`~/.local/state/omarchy-buzz/delivery/ledger.json`). Preserve it on upgrade and
ordinary uninstall: it records request/event bindings used to prevent duplicate
publication. Its lock is released when the helper exits. Ledger failure disables
sending while read-only connectivity remains available. No message bodies or
private keys are stored there.
