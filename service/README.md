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
and four user units (messaging service/socket and agent-manager service/socket),
plus `scripts/agent-login` under `~/.local/share/omarchy-buzz/scripts/`.
A sidecar detects accidental corruption; it does not prove
who created the package. Only install an artifact you built or obtained through
a trusted, independently verified channel. `install` performs no download;
`fetch` (below) downloads from this project's GitHub Releases.
`--dry-run` checks package structure and previews installed paths without running
the new binary. Actual installation first runs its bounded `--version` command
from a private temporary directory and checks the reported version and Buzz pin;
this checks target loadability before stopping the old service.

The same `install` command upgrades an existing complete installation. It
rejects symlinks, partial installations, edited units, unexpected directories,
and architecture mismatches. It stops the old helper before replacement and
keeps copies of the managed files that already exist under
`~/.local/share/omarchy-buzz/backups/`. The tool reloads systemd and enables
both sockets. If activation fails, it restores the old files and prior socket
state. Both the messaging and agent-manager sockets are enabled. Upgrades from
the earlier messaging-only installation add the agent-manager files and socket.
Review an error before retrying; no configuration or identity is changed.

Before uninstalling, stop and delete managed agents through the panel while the
agent manager is available. Deletion stops/disables their generated units and
keeps the identity and workspace by default. For enrolled agents, deletion also
requests removal from their relay rooms; a failed removal must be resolved
before treating deletion as complete. Separately
configured room agents require their own removal procedure. The helper
uninstaller does not remove per-agent units, agent bundles, or provider profiles.

To remove the helper, its four owned user units, and installed sign-in script:

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
removed managed files and leaves configuration, Secret Service identity, and the
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

## Install a release

Releases on <https://github.com/randymy/omarchy-buzz/releases> carry prebuilt
helper packages for ARM64 (`aarch64`) and x86-64 (`x86_64`). From a checkout of
the matching tag (the installer checks the package against the checkout's
version, Buzz pins and reviewed scripts):

```bash
python3 scripts/helper-install fetch --dry-run 0.0.21
python3 scripts/helper-install fetch 0.0.21
```

`fetch` downloads only this machine's `omarchy-buzz-<version>-linux-<arch>.tar.gz`
and its `.sha256.json` sidecar into `~/.cache/omarchy-buzz/helper/releases/<version>/`
(HTTPS only, redirects only to GitHub's asset hosts, bounded sizes, a 60-second
limit; the files are saved owner-readable and never made executable), then runs
the same checks and installation as `install`. To verify a release by hand,
download its assets into one directory, including
`omarchy-buzz-<version>-checksums.txt`, and run (add `--ignore-missing` if you
downloaded only some of them):

```bash
sha256sum -c omarchy-buzz-0.0.21-checksums.txt
```

The checksums and sidecars detect corruption; the assets are not signed, so
they do not prove who built them. Each release's `build-<arch>.json` names the
source commit and workflow run of its binary.

## Agent manager installation scope

`omarchy-buzz-agents.service` and `omarchy-buzz-agents.socket` supervise
`omarchy-buzz agents-daemon` on `%t/omarchy-buzz/agents.sock`, and
`agent.service.in` is the reviewed template from which that daemon writes one
`omarchy-buzz-agent-<id>.service` per agent (see
[docs/AGENTS_SERVICE.md](../docs/AGENTS_SERVICE.md)). The current
`scripts/helper-install` installs and enables the agent-manager socket alongside
the messaging socket, and its uninstall command stops and removes those manager
units. Installing the manager does not install harness bundles, sign in to a
provider, enroll an agent identity, or create and start an individual agent.
Those operations require separate setup and explicit actions in the panel.

Release 0.0.13 passed ARM64 CI and a recorded installed manager/bridge check.
Creating and starting an individual agent through the panel, including real
identity enrollment and relay publication, still require deployment acceptance.
Treat the earlier stock Codex room-agent result separately from acceptance of
the new manager. Review the implementation limitations in
[AGENTS_SERVICE.md](../docs/AGENTS_SERVICE.md) before enabling individual agents.
