# Local helper packages

Helper release artifacts are not yet published. These are development preview packages, not a claim of public release readiness. The repository provides a local packaging tool; it does not install, tag, upload or publish anything. Upgrade the Omarchy plugin and helper together: plugin updates alone do not replace the helper binary.

Build the helper using the repository's documented Rust toolchain and locked dependencies, then package the built executable:

On Arch, install `pkgconf` and `dbus` before building. The intended system
D-Bus linkage requires `libdbus-1.so.3` on the target machine, alongside a
working session D-Bus and Secret Service provider for identity access. A fresh
ARM64 CI build and its isolated keyring test passed in
[run 36621010979](https://github.com/randymy/omarchy-buzz/actions/runs/36621010979)
at source `8928adc`. Target-machine archive, linkage and keyring verification
have also passed on Omarchy, using temporary sockets and a private Secret Service with synthetic identities. The earlier ARM64 package evidence describes the old vendored
build. The source inventory and its review flags have not been remeasured for a
new binary.

```sh
cargo build --release --locked --manifest-path helper/Cargo.toml
python3 scripts/package-helper helper/target/release/omarchy-buzz artifacts/local
python3 tests/package_helper.py
```

When using an alternate Cargo target directory, pass its actual binary path. The output directory must not contain the generated filenames already. Only Linux ELF64 x86_64 and aarch64 binaries are supported; architecture is read from the ELF header. The tool executes the supplied local binary with `--version`, so supply only a binary you built and trust. ELF architecture checks do not prove libc compatibility or runtime dependencies; test the archive on its intended distribution before any release.

The deterministic tar.gz contains the helper executable, Apache license, both systemd user units and their README, helper README, and `version.json`. With `--notices PATH`, it also contains the generated third-party inventory and original notice texts. See [notice generation](DEPENDENCY_NOTICES.md). Packaging checks target architecture, current lockfile SHA256, package source/checksum associations and every included notice hash. Version metadata reports whether notices were included, the flagged-package count, and that review remains required. It includes no user configuration, identity keys, delivery ledger or runtime data. Archive timestamps and ownership are normalized. Version metadata must match the plugin manifest, helper crate, both pinned Buzz dependencies and the compatibility constant. Packaging does not attest that a build used those dependencies; the build and test process must establish that separately.

The accompanying `.sha256.json` records archive filename, size, SHA256 and version metadata, including `developmentPreview: true`. SHA256 detects accidental corruption when compared against a trusted expected digest; it is not a signature or proof of publisher identity. No automatic installation script is included. Review the documented service setup and archive contents before manually installing. Packaging tests use synthetic ELF headers and injected version metadata without executing the fixtures or contacting a network.

Before public distribution, review the bundled binary's third-party dependencies and required license and notice texts, resolve missing evidence, review distribution obligations, and include the appropriate notices in the artifact. Packaging without `--notices` includes only the repository's root license and explicitly reports missing third-party inventory. Including a generated inventory does not complete legal review or establish that all required notices were found. This remains a public distribution gate alongside target-machine validation and release review.
