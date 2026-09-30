# Prepared community submission

Status: draft for owner review, not submitted. On September 30, 2026, GitHub
reported this repository as private. Public access is required before submission;
the first checklist item must not be checked until that changes. Confirm all
checklist statements and approve the final title/body before creating the issue,
as required by the [submission guide](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md).

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
- Existing successful ARM64 helper CI covers `44c3e2c`, before the agent-manager
  merges. It does not establish acceptance of the current full checkout. Do not
  describe the current source as a tested messaging-only release: its installer
  also enables the agent-manager socket. Disclose that experimental scope.
- No root preview is present. It is optional; the marketplace can use a fallback.

## Issue draft

Title: **[Plugin]: Buzz for Omarchy**

The six headings and checklist below match the required submission format. All
boxes remain unchecked in this draft pending owner confirmation and public
repository access. Only submit once all five can truthfully be checked.

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
Block or Omarchy product. The manifest and helper report version 0.0.12. Native
bar, panel and normal-window messaging includes authenticated room discovery,
bounded history and thread replies, plain-text sending, exact-key mentions,
direct messages and optional generic activity notifications. Activity counts
are local observations, not synchronized unread counts.

Please review as manual setup: messaging requires a separately built Rust
helper, user systemd units, Linux Secret Service and an existing Buzz identity
with access to a configured hosted or self-hosted community. The standard
Omarchy plugin command installs only the UI checkout. The helper installer
also installs the experimental agent-manager service/socket and sign-in script
and enables both messaging and agent-manager sockets. Agent bundles, provider
sign-in, enrollment and individual agent creation/start require separate setup
and explicit actions. The new manager's real deployment acceptance is pending;
earlier stock Codex room-agent evidence is a separate result.

Installation, upgrade, removal, dependency notices and limitations are documented
in README.md, service/README.md and docs/AGENTS_SERVICE.md. Plugin removal alone
does not uninstall the helper. Helper removal does not remove individual agent
units or bundles; those must be handled separately. Identities, provider profiles,
configuration, workspaces and the delivery ledger are preserved by ordinary
helper removal. No preview asset is submitted.

### Submission checklist

- [ ] The repository is public and contains installation and removal instructions.
- [ ] I have documented the plugin license and any external dependencies.
- [ ] I confirm that I own or have permission to submit this plugin and its preview assets.
- [ ] The plugin does not overwrite user configuration without explicit consent.
- [ ] I understand that approval is for listing and is not a security review.
