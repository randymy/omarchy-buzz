# Prepared community submission

Status: prepared for submission. The owner authorized public access and the
repository was made public on September 30, 2026. Submit only while all checklist
statements are true, using the approved title and body, as required by the [submission guide](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md).

The marketplace accepts a submission issue, then performs exact-commit validation
and maintainer review. Do not open a registry PR or edit generated catalog files
to bypass that process. Request the `manual-setup` designation because the
separate helper build and installation are mandatory for messaging.

## Listing choices and review

- Category: **Productivity**, matching all three current Slack listings
  (`io.github.thisisgm.slack`, `bottelet.slack`, `janrenz.omarchy.slack`).
- Tags: **bar, quickshell**, also matching those listings.
- Recommended display name: **Buzz (unofficial)**, to identify this independently
  maintained integration. The current manifest name, **Buzz for Omarchy**, is
  also valid; no marketplace rule requires renaming it. The marketplace derives
  its display name from `manifest.json`, not the issue title. If the owner adopts
  the recommendation, change only `name` there and the issue title below; keep
  the permanent plugin ID `community.buzz` and the short bar label `Buzz`.
- No active or retired `community.buzz` ID, source-repository listing, or existing
  submission/PR for this repository was found in the September 30 review. Recheck
  immediately before submitting.
- Local preflight of source commit
  `6b2ed118c44458de463c2a9c1214250734594401` passed manifest/entry-point,
  root README/license, repository-layout and ID-uniqueness checks. Applying the
  marketplace's static analysis to its 43 selected local source files produced
  `review-required`, with no blocking findings: remote build, service management,
  installer and package-manager capabilities. The package-manager evidence is in
  the isolated relay test-container recipe. This is a local preflight, not an
  official exact-commit marketplace scan or security approval.
- ARM64 helper CI for release `0.0.13` at `7b1ce8a` passed in
  [run 36731957484](https://github.com/randymy/omarchy-buzz/actions/runs/36731957484).
  The handoff records an installed manager/bridge check; first-agent creation,
  enrollment and start through the panel remain pending. Disclose that scope.
- No root preview is present. It is optional; the marketplace can use a fallback.

## Issue draft

Title: **[Plugin]: Buzz for Omarchy**

The six headings and checklist below match the required submission format.
The owner authorized submission and public repository access. Recheck the
repository and metadata immediately before opening the issue.

---

### Repository URL

https://github.com/randymy/omarchy-buzz

### Category

Productivity

### Tags

bar, quickshell

### Suggest a missing tag

_No response_

### Maintainer notes

Independently maintained development preview for Block's Buzz, not an official
Block or Omarchy product. The manifest and helper report version 0.0.13. Native
bar, panel and normal-window messaging includes authenticated room discovery,
bounded history and thread replies, plain-text sending, exact-key mentions,
direct messages and desktop notifications for mentions, DMs and threads (configurable, with an option to omit message text). Activity counts
are local observations, not synchronized unread counts.

Please review as manual setup: messaging requires a separately built Rust
helper, user systemd units, Linux Secret Service and an existing Buzz identity
with access to a configured hosted or self-hosted community. The standard
Omarchy plugin command installs only the UI checkout. The helper installer
also installs the experimental agent-manager service/socket and sign-in script
and enables both messaging and agent-manager sockets. Agent bundles, provider
sign-in, enrollment and individual agent creation/start require separate setup
and explicit actions. Release 0.0.13 passed ARM64 CI and a recorded installed
manager/bridge check. Creating, enrolling and starting the first agent through
the panel still need real deployment acceptance; earlier stock Codex room-agent
evidence is a separate result.

Installation, upgrade, removal, dependency notices and limitations are documented
in README.md, service/README.md and docs/AGENTS_SERVICE.md. Plugin removal alone
does not uninstall the helper. Helper removal does not remove individual agent
units or bundles; those must be handled separately. Identities, provider profiles,
configuration, workspaces and the delivery ledger are preserved by ordinary
helper removal. No preview asset is submitted.

### Submission checklist

- [x] The repository is public and contains installation and removal instructions.
- [x] I have documented the plugin license and any external dependencies.
- [x] I confirm that I own or have permission to submit this plugin and its preview assets.
- [x] The plugin does not overwrite user configuration without explicit consent.
- [x] I understand that approval is for listing and is not a security review.
