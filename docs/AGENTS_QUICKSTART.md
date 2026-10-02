# Agents quickstart: a fresh Omarchy machine

Yo, human. This page takes you from a plain Omarchy install to an agent that
answers in a Buzz room using **your own** Claude Code or Codex subscription.
It is a development preview: every step below is real, but nobody has yet run
the whole path end to end on a fresh machine (see "Not verified" at the end).

Two facts before you start:

- Agents run **only on this machine**, under your user. When it is off, they are off.
- Each harness has **one shared provider login** on this machine. Every Claude
  Code agent you create uses the same Claude login, and every Codex agent uses
  the same ChatGPT login. Every answer, and every **Test model** probe, is a
  real model turn that counts toward that subscription's usage.

## 1. Install the plugin

Install **Buzz for Omarchy** (`community.buzz`) from the Omarchy plugin
marketplace, or add it from its repository:

```bash
omarchy plugin add https://github.com/randymy/omarchy-buzz.git --enable
```

The plugin is only the bar widget and panel. It does not install the helper;
the marketplace lists it as "manual setup" for that reason. The steps below
assume the plugin's checkout is at `~/.config/omarchy/plugins/community.buzz`:

```bash
PLUGIN=~/.config/omarchy/plugins/community.buzz
grep '"version"' "$PLUGIN/manifest.json"
```

## 2. Install the helper from a release

Releases carry prebuilt helpers for ARM64 and x86-64. Use the version the
plugin's `manifest.json` reports; the installer refuses a package whose
version, Buzz pins or scripts differ from that checkout.

```bash
python3 "$PLUGIN/scripts/helper-install" fetch --dry-run <version>
python3 "$PLUGIN/scripts/helper-install" fetch <version>
```

`fetch` downloads only this machine's package and its `.sha256.json` from the
project's GitHub Releases into `~/.cache/omarchy-buzz/helper/releases/<version>/`,
checks size, checksum, architecture and version, runs the new binary's
`--version` in a private temporary directory, then installs:

- `~/.local/bin/omarchy-buzz`;
- four user units in `~/.config/systemd/user/`: `omarchy-buzz.service`/`.socket`
  (messaging) and `omarchy-buzz-agents.service`/`.socket` (agent manager);
- the agent scripts (`agent-login`, `agent-bundle`, `room-agent`, `room-sandbox`,
  `room-agent-entry`, `room-codex`, `room-codex-acp`, `room-claude`,
  `room-claude-acp`) in `~/.local/share/omarchy-buzz/scripts/`.

It then runs `systemctl --user daemon-reload` and enables **both** sockets,
including the agent manager's. It does not build bundles, sign you in or create
agents. You also need a working Secret Service (keyring); agents need
`/usr/bin/bwrap` (bubblewrap) and `/usr/bin/secret-tool`.

## 3. Join the community

**The owner** (or a relay admin) opens Settings → **Invite people**, picks
1/5/25 people and 1/7/30 days, clicks **Create invite** and sends you the link
or the ready-made message (each has **Copy**). Only the relay's owner or admins
can create invites.

**You** open the panel. First setup offers **Join an existing community**:
paste the link, click **Join community**, then **Create a new identity on this
device** (the invite is redeemed right after). If the community has terms, read
them and accept. The invite makes you a member of the relay, not of any room.

You land on "Welcome to <community>": **Pick a room to get started.** Open rooms
join with one click and no approval. A private room needs someone already in it
to add you. Later communities: account menu → **Join an existing community…**.

## 4. Build the harness bundle

An agent runs from a self-contained bundle in
`~/.local/share/omarchy-buzz/agent-<harness>/`, assembled by `agent-bundle` from
pinned, hash-checked inputs. Run it **from the plugin checkout** (it needs the
checkout's `packaging/` and `LICENSE`). It never replaces an existing bundle.

**ARM64 and x86-64.** Bundles are built for the machine's own architecture,
and `bundle.json` records it (a bundle without that field predates x86-64
support and is ARM64). The script refuses a Buzz binary, Node or Claude CLI
built for another architecture (`buzz_binary_wrong_architecture`,
`node_wrong_architecture`, `claude_cli_wrong_architecture`), a CI artifact
whose `build.json` names another architecture (`buzz_architecture_mismatch`),
and a bundle assembled elsewhere (`bundle_architecture_mismatch`).

You need, already on disk:

- **Official Buzz binaries** `buzz-acp`, `buzz`, `buzz-admin` at Buzz
  `781d395`: the `bin/` of the `stock-agent-arm64` or `stock-agent-x86_64`
  artifact from the repository's manual workflow "Manual stock room-agent
  binaries (ARM64, x86-64)" (kept 14 days). With its `build.json` beside `bin/`,
  that file must name `781d395`, no patches and this machine's architecture.
  Without it, the files must match the hashes of run 36639519388 (ARM64) or
  37077396263 (x86-64).
- **Node `v22.23.3`** for this architecture (the script runs `node --version`),
  and its `npm-cli.js` for the locked adapter install. The official
  `node-v22.23.3-linux-arm64` or `-linux-x64` tarball from nodejs.org has both.
- **Claude Code only:** Claude Code **exactly 2.1.280**. The default path is the
  mise install, `~/.local/share/mise/installs/claude/latest/claude`; another file
  can be given with `--claude-cli`. The script checks its SHA-256 (ARM64
  `92f2b4fd…45a2`, x86-64 `1e08503d…925b`, the `claude` binary in
  `@anthropic-ai/claude-agent-sdk-linux-arm64`/`-linux-x64` 0.3.280), so **any
  other Claude Code version fails** with `claude_cli_unpinned`.

Codex needs no separate CLI: its native binary comes in the pinned npm packages
(`@openai/codex` 0.158.0 and its `linux-arm64` or `linux-x64` native package). `--npm-ci` runs a locked `npm ci --ignore-scripts`
against registry.npmjs.org; it is the only step that downloads.

```bash
# Claude Code
python3 "$PLUGIN/scripts/agent-bundle" claude-code \
  --buzz-bin /path/to/stock-agent/bin --node /path/to/node \
  --npm-ci --npm-cli /path/to/npm-cli.js

# Codex
python3 "$PLUGIN/scripts/agent-bundle" codex \
  --buzz-bin /path/to/stock-agent/bin --node /path/to/node \
  --npm-ci --npm-cli /path/to/npm-cli.js
```

Instead of `--npm-ci --npm-cli`, `--adapter DIR` takes an npm tree you prepared
yourself; versions and integrity are checked either way. Then check it:

```bash
~/.local/share/omarchy-buzz/scripts/agent-bundle claude-code --check
```

- `ready`, exit 0: good to go.
- `stale`, exit 3, `{"error": "launcher_outdated"}` on stderr: the bundle's
  launcher scripts differ from the installed ones. Click **Refresh bundle** in
  the panel, or run the same command with `--refresh-launcher`.
- `missing`, exit 1, `{"error": <category>}` on stderr (for example
  `bundle_missing`, `bundle_hash_mismatch`): rebuild. Move the old bundle
  aside first; assembly will not overwrite it.

## 5. Sign in with your subscription

In the agent editor, while a harness is signed out, a **Sign in to Claude
Code** (or **Sign in to Codex**) button shows. It runs `agent-login <harness>`,
which opens a floating Omarchy terminal (else `xdg-terminal-exec`) running the
bundle's own CLI:

- Claude Code: `claude --settings <forceLoginMethod=claudeai> auth login --claudeai`
- Codex: `codex -c forced_login_method="chatgpt" login`

Finish the login in that terminal and browser. The login lives in the shared
profile `~/.local/state/omarchy-buzz-agent-preview/<harness>/`, used by every
agent of that harness. `agent-login --status <harness>` only checks that the
credential file exists and is not empty; it never opens it, so an expired or
revoked login still reads `signed-in`. **Test model** is the real check.

## 6. Create the agent

1. Panel → **Agents** → **+ New agent**. The agent belongs to the community
   that is active now.
2. Fill in **Name**, **Description** (public profile), **Instructions**, the
   **Harness** (Claude Code or Codex) and optionally a **Model** (empty =
   harness default; Claude Code takes `opus`, `sonnet`, `haiku`, `fable` or a
   `claude-…` id).
3. Tick 1 to 8 **Rooms** you have already joined. **Answers**: **Only me**
   (default) or **Anyone who mentions it**. Optional: **Answers direct
   messages**, **Workspace** (default
   `~/.local/state/omarchy-buzz-room-workspaces/<id>`), **Start at login**.
4. **Create**, then **Enroll**: this makes the agent's own identity (kept in
   Secret Service) and adds it to its rooms, signed with your key.
5. Sign in if you have not (step 5), optionally **Test model**, then **Start**.
   Mention the agent in one of its rooms.

## 7. What the community owner may need to do

Enroll publishes, signed by **you** (the agent's owner), the agent's records
and one kind 9000 add-member event per room. The relay accepts these only if
you may add members to that room.

What this repository says about that: the relay lets a channel owner or admin
remove any member (kind 9001), and a plain member remove an agent it owns. For
**adding** (kind 9000) it records only that in a private room an active member
can add someone, and that a non-member can add themselves to an open room. It
does not state whether a plain member may add another identity, such as an
agent, to an open room, and no non-owner enrollment has been tried live.

So if **Enroll** fails (`enroll_failed`) for a room, ask the owner to make you
an owner or admin of that room in Buzz Desktop, or pick a room you own, then
retry: **Enroll** again while the agent has no identity yet, otherwise
**Retry enrollment** on this community's line under **Communities** in the
editor. A retry reuses the agent's identity and adds only rooms not yet
acknowledged.

## 8. The same agent in another community

One agent can be in up to four communities with the same identity. Switch to
the other community (you must be a member there and signed in), open the agent
from **In other communities**, tick 1 to 8 of that community's rooms and click
**Add to <community>**. The owner step above applies there too. Each community
gets its own unit and workspace; **Leave <community>** removes the agent from
one of them (never the last: delete the agent instead).

## Not verified

- The whole path end to end on a fresh machine, by someone other than the maintainer.
- That a marketplace install puts the checkout at `~/.config/omarchy/plugins/community.buzz`.
- That a published release exists for the version your plugin reports.
- That plain Omarchy ships `bwrap` and `secret-tool`.
- How a newcomer gets the `stock-agent-arm64` artifact (it is made only by a
  manual workflow run on the repository), or whether a fork's run works.
- Where to get Node v22.23.3 and Claude Code 2.1.280; the repo names neither a
  download source nor a mise command.
- Any agent bundle on x86-64 (refused by the script as written).
- Whether the relay accepts a kind 9000 add-member from a plain member, in an
  open or a private room; whether a relay owner/admin who is not the channel's
  owner may add one.
- Whether the relay accepts an agent key that is not itself a relay member
  (NIP-43); the docs describe only the owner's attestation in the agent's AUTH.
