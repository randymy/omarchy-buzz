# Local helper packages

Helper release artifacts are not yet published. These are development preview packages, not a claim of public release readiness. The repository provides a local packaging tool; it does not install, tag, upload or publish anything. Upgrade the Omarchy plugin and helper together: plugin updates alone do not replace the helper binary.

Build the helper using the repository's documented Rust toolchain and locked dependencies, then package the built executable:

```sh
cargo build --release --locked --manifest-path helper/Cargo.toml
python3 scripts/package-helper helper/target/release/omarchy-buzz artifacts/local
python3 tests/package_helper.py
```

When using an alternate Cargo target directory, pass its actual binary path. The output directory must not contain the generated filenames already. Only Linux ELF64 x86_64 and aarch64 binaries are supported; architecture is read from the ELF header. The tool executes the supplied local binary with `--version`, so supply only a binary you built and trust. ELF architecture checks do not prove libc compatibility or runtime dependencies; test the archive on its intended distribution before any release.

The deterministic tar.gz contains the helper executable, Apache license, both systemd user units and their README, helper README, and `version.json`. It includes no user configuration, identity keys, delivery ledger or runtime data. Archive timestamps and ownership are normalized. Version metadata must match the plugin manifest, helper crate, both pinned Buzz dependencies and the compatibility constant. Packaging does not attest that a build used those dependencies; the build and test process must establish that separately.

The accompanying `.sha256.json` records archive filename, size, SHA256 and version metadata, including `developmentPreview: true`. SHA256 detects accidental corruption when compared against a trusted expected digest; it is not a signature or proof of publisher identity. No automatic installation script is included. Review the documented service setup and archive contents before manually installing. Packaging tests use synthetic ELF headers and injected version metadata without executing the fixtures or contacting a network.

Before public distribution, inventory the bundled binary's third-party dependencies and required license and notice texts, review distribution obligations, and include the appropriate notices in the artifact. The current archive includes only the repository's root license; it does not claim to complete that third-party notice inventory. This remains a public distribution gate alongside target-machine validation and release review.
