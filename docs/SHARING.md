# Share Buzz for Omarchy

Share https://github.com/randymy/omarchy-buzz with another Omarchy user.
The current version is a messaging development preview, not a marketplace
release. Follow the repository README for both the native plugin and the
separate helper; installing only the plugin does not provide connectivity.
The helper can be built on Linux for the recipient's architecture. Do not
copy an ARM64 binary onto an x86-64 computer.

Each participant uses their own Buzz identity. Share the community relay URL
and invite that identity using Buzz's normal membership controls. Do not share
your private identity key, Secret Service data, provider login, or helper state.
Plugin installation does not grant community or private-room membership.

For a relay reachable only through Tailscale, the recipient also needs permitted
network access to that machine and relay port. Giving someone the relay URL
alone does not provide that access. A hosted Buzz community can be used instead;
the plugin supports either configuration.

People on macOS or other platforms can use upstream Buzz Desktop in the same
community and room. They do not need Omarchy or this plugin. Room-agent execution
is not included in this preview.

The optional desktop launcher is described in [NATIVE_LAUNCHER.md](NATIVE_LAUNCHER.md).
Installation and removal are in the [README](../README.md) and
[helper installer instructions](../service/README.md). Marketplace submission
and downloadable release packages remain separate distribution work.
