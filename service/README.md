# Helper supervision (development)

These user units are under development and are not installed by the QML plugin.
The UI connects through a bridge and displays connection status; messaging remains unavailable.

The socket activates `%h/.local/bin/omarchy-buzz daemon`. It retains its private
runtime directory while the daemon exits after its last client disconnects.
Normal idle exits do not restart the daemon. Failed processes restart at most
three times per minute. Memory, task, descriptor, and shutdown limits constrain
helper failures; they do not make the shared Omarchy QML process a sandbox.

After building and testing the helper, install its binary at
`~/.local/bin/omarchy-buzz` and these two unit files at
`~/.config/systemd/user/`. Review existing files before replacing them. Then:

```bash
systemctl --user daemon-reload
systemctl --user enable --now omarchy-buzz.socket
```

No relay connection is attempted without an explicitly enrolled identity.
Enrollment requires the user's Secret Service, accessed through session D-Bus.
The service deliberately does not accept credential environment files. Do not
enable upstream payload logging in a unit override.

After changing helper configuration, select Retry in the panel. A service restart
is an alternative when no UI is connected.

To uninstall, first disable/remove the QML plugin, then:

```bash
systemctl --user disable --now omarchy-buzz.socket
systemctl --user stop omarchy-buzz.service
```

Remove only this project's two installed unit files and helper binary, then run
`systemctl --user daemon-reload`. Preserve configuration and Secret Service
entries unless the user explicitly chooses to delete them. Omarchy's native
plugin removal cannot remove these separately installed files.

Validation so far: installed systemd socket activation passed with unconfigured
public status, and an isolated inherited-socket test passed idle exit/reactivation.
Secret Service enrollment/retrieval passed in a separate private test session.
Keyring access under every service hardening setting and resource limits under
hostile relay load still require validation.
