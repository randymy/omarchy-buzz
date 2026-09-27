# Deployed relay compatibility — 2026-09-27

These are anonymous, read-only observations, not authenticated deployment
certification. No production identity was enrolled, no conversation was read,
no event was sent, and no relay configuration was modified. Private deployment
addresses and signing keys are intentionally omitted.

## Self-hosted deployment

The operator-provided tailnet hostname responds on HTTP port 3000:

- `GET /info`: 200; software identifies Block Buzz, version `0.2.0`.
- `self`: structurally valid 64-character hexadecimal signing identity.
- Advertised NIPs include 29, 42 and 43; extensions include `nip-er`, `nip-pl`.
- Authentication is required and writes are restricted.
- `GET /_readiness`: 200, `status: ready`.
- `GET /_status` on the main listener: 404; no exact build revision obtained.
- Standard HTTPS port 443 and HTTP port 80 refused connections during inspection.

The helper intentionally rejects remote `ws://` origins. Tailscale can provide
encrypted transport, but the helper does not independently establish that a
hostname resolves through an authenticated VPN path. Do not add a blanket
`*.ts.net` or private-IP exception. Establish a supported HTTPS/WSS origin.
The advertised version differs from the inspected source pin; package numbers
and NIP lists do not establish NIP-CW, signed roster, or authorization behavior.

## Hosted setup and metadata

[Official hosted-community guidance](https://block.github.io/buzz/support.html)
requires a specific community URL and an invited identity. The generic public
website is a setup portal, not a universal relay URL. Keep the existing hosted
handoff; do not scrape a desktop account or import its secrets automatically.

The upstream-documented onboarding origin
`https://onboarding.communities.buzz.xyz/info` returned JSON over valid TLS:
version `0.2.1`, a structurally valid `self`, and extensions `nip-er`, `buzz-gif`.
It is a public discovery witness, not an enrollment target or the user's chosen
community. Authenticated hosted compatibility remains unverified.

Its metadata was 35,782 bytes, including an approximately 35 KB inline icon.
That exceeded the helper's 32 KiB NIP-11 limit. The fix raises the total document
budget to 128 KiB while retaining content-length and streamed-byte enforcement,
no redirects, request deadlines and signer validation. Only the signer is
returned; the icon is neither rendered nor fetched. Synthetic tests exercise a
36 KiB icon over HTTP, exact-budget acceptance, one-byte-over-budget rejection,
and signer pin mismatch. No live payload is used as a test fixture.

## Proposed HTTPS deployment work

Prefer a tailnet-only TLS endpoint using the operator's existing reverse proxy
or [Tailscale Serve](https://tailscale.com/docs/reference/tailscale-cli/serve).
Do not enable public Funnel. Before changing the Mac:

1. Obtain its existing SSH connection/account and inspect the deployment's exact
   Buzz build and Tailscale installation. No usable matching SSH alias was found
   locally; the workstation's system SSH config also reports an ownership/mode
   error. No SSH authentication or remote changes have been attempted.
2. Inspect existing Serve/proxy configuration and retain a rollback copy. Check
   whether port 443 is available and certificate provisioning is permitted.
3. Verify the new external authority maps to the same Buzz community. Preserve
   correct Host, scheme and signed NIP-98 URL handling; an origin change is not
   merely a port forward. Do not overwrite existing host mappings blindly.
4. Prepare a scoped HTTPS proxy to the existing loopback backend. If Serve is
   suitable and no conflicting rule exists, its documented command shape is
   `tailscale serve --bg --https=443 http://127.0.0.1:3000`. This is a proposal,
   not a command executed or a claim about installed macOS support.
5. Check certificate trust, `/info`, signer continuity and readiness over HTTPS.
   Then validate the chosen user's authenticated read path and native panel.
   Enrollment uses the helper's hidden terminal/Secret Service path; private
   keys must never be supplied in chat, command arguments or QML.

Rollback must remove only the added proxy rule and restore any specifically
changed origin configuration. Avoid a global `tailscale serve reset` that could
remove unrelated services. Real send and agent workflows require a deliberately
chosen room and the existing authority boundaries; production conversations
must never become conformance fixtures.
