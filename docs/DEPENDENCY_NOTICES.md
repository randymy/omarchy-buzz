# Dependency notice inventory

`scripts/helper-notices` generates an offline source inventory from locked Cargo metadata and cached package sources. It preserves declared license expressions and copies original license, copying, copyright and notice texts, including nested vendored notices. Texts are deduplicated by SHA256. The inventory records package name/version/source, registry checksum from Cargo.lock, source manifest SHA256, each notice's original relative path and SHA256, and review flags. It contains no absolute cache paths or user configuration.

Generate metadata for the intended Linux target using the build's Cargo/toolchain and features. For the default helper features:

```sh
cargo metadata --offline --locked --format-version 1 \
  --filter-platform aarch64-unknown-linux-gnu \
  --manifest-path helper/Cargo.toml > /tmp/buzz-license-metadata.json
python3 scripts/helper-notices /tmp/buzz-license-metadata.json \
  --lock helper/Cargo.lock --target aarch64-unknown-linux-gnu \
  --output artifacts/notices-arm64
python3 scripts/package-helper /path/to/built/omarchy-buzz artifacts/local \
  --notices artifacts/notices-arm64
python3 tests/helper_notices.py
```

To include the pinned `nostr 0.44.8` repository-root license text in a newly generated inventory, add `--supplemental docs/evidence/notices` to the `helper-notices` command. This is opt-in. The generator checks the evidence file's size and SHA256, the locked registry checksum, the cached manifest SHA256, and the cached `.cargo_vcs_info.json` hash, revision and path. It checks the source URL's host and revision/path pattern, but does not independently bind that URL's repository to the package; that association remains for review. It rejects symlinks, extra files, packages absent from the resolved graph, and mismatches. The resulting package retains `supplemental_notice_review`; including the text does not clear the review gate.

Use `x86_64-unknown-linux-gnu` for that target. The supplied target labels the metadata's generation context; the generator cannot prove the command that produced metadata, its feature selection or a binary's actual dependency linkage. Use a fresh `--locked` metadata result from the same build context. Unfiltered metadata can require uncached crates for unrelated platforms; this tool does not download missing sources. It does not execute dependency build scripts.

The resolved non-dev runtime/build graph is deliberately conservative. Cargo feature unification and build dependencies can make it overinclusive; it is not a linked-binary SBOM. Dev-only edges are excluded. Git packages use the cached checkout's repository license/notice files, including shared upstream notices. Registry packages use their own unpacked source root. Symlink notices are flagged rather than copied. Missing notice text, missing declared licenses, compound expressions and legacy slash expressions remain explicit review flags. No license alternative is selected automatically. Other declared expressions are retained verbatim, not certified as valid SPDX or acceptable for distribution.

On 2026-09-26, the then-locked default-feature ARM64 metadata yielded **252 dependencies**, **19 flagged packages**, and under **1 MB** of deduplicated inventory/notices. Five packages lacked cached notice files: `nostr`, `bitcoin_hashes`, `bitcoin-internals`, `bitcoin-io`, and `bitcoin-consensus-encoding`. Their MIT/CC0 declarations are recorded but do not supply absent notice text. Compound/legacy flags include AWS-LC, ring, OpenSSL, Unicode and Rustix-related packages. These are evidence requiring review, not findings of infringement. That result was specific to lock SHA256 `e3a535bc0dc0f938818a115e828348c8000381767f6f64d46cb96c3c2e12e433`; rerun after any lock or feature/target change.

Before public distribution, review missing evidence and all applicable license obligations, including source copyright/license headers not named as notice files, vendored/generated components, toolchain/runtime libraries and dynamically linked system dependencies. The filename inventory cannot establish these are complete. Determine the necessary attribution, license choices, notice preservation, source availability and any other obligations for the actual binary and distribution. Packaging includes the evidence and unresolved count; it always marks review required and gives no legal assurance or distribution approval.

An additional provenance pass found that the cached `nostr 0.44.8` `.cargo_vcs_info.json` records commit `a86ce27c3b4d0dcab186a707336237653a01b114` and `crates/nostr`; its declared repository is `https://github.com/rust-nostr/nostr.git`. The repository-root [LICENSE at that exact commit](https://github.com/rust-nostr/nostr/blob/a86ce27c3b4d0dcab186a707336237653a01b114/LICENSE) was preserved in `docs/evidence/notices/nostr-0.44.8-LICENSE.txt` (1122 bytes; SHA256 `a333d394b9f31b6ca64d08f3048a8a38125c181d68d3d376c4ddf988cffd12d2`). The adjacent `evidence.json` pins its source URL, revision, text hash, locked registry checksum, manifest hash and cached VCS metadata hash. This remains review evidence only.

The four missing Bitcoin-crate texts remain unresolved: their cached published source packages do not contain `.cargo_vcs_info.json`, so this pass could not establish an exact upstream commit. The exact cached `.crate` artifacts match the Cargo.lock checksums, and their published manifests declare `CC0-1.0`; `bitcoin_hashes`, `bitcoin-internals`, and `bitcoin-consensus-encoding` also carry `SPDX-License-Identifier: CC0-1.0` in `src/lib.rs`. These observations support the declarations but do not supply the missing license texts. No text was substituted from a similarly named version or guessed release tag. The other fourteen compound/legacy expression flags also remain for review. The supplemental MIT text does not by itself establish that all notices or obligations for the nostr crate and binary are satisfied.
