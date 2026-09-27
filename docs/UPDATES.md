# Following Buzz updates

The helper uses the official `block/buzz` repository at explicit full commit
revisions. `scripts/check-upstream` compares both Rust dependency pins with
`GET https://api.github.com/repos/block/buzz/commits/HEAD`, requesting GitHub’s
SHA response media type so commit patch size does not affect the check. It follows hosted and
custom relay software equally; a new upstream source commit does not prove a
particular deployed relay upgraded or remains compatible.

The GitHub workflow is currently manual-only to reduce development notifications.
Its daily 09:17 UTC schedule is paused by user request. A manual run reports the current pin and
upstream default-branch HEAD in its job summary. When they differ, it prepares
a draft dependency-update PR. Manual dispatch can disable candidate preparation
and perform only the check. Schedules run on the repository's default branch;
this automation starts after the workflow is merged and enabled there.

Candidate preparation changes only `buzz-sdk` and `buzz-ws-client` revisions,
`compatibility::BUZZ_REVISION`, and the resolved Cargo.lock. Historical design
inspection revisions remain immutable. The script requires matching existing
pins and validates the new full hexadecimal SHA before editing. Each file
replacement is atomic and ordinary write failures roll back both source files;
a process crash between replacements requires inspecting/recovering mismatched
pins, not assuming a multi-file transaction succeeded.

The candidate workflow resolves dependencies and runs formatting, Rust tests,
build, helper/plugin version and backend-pin diagnostics, isolated helper smoke
and socket activation checks, and private synthetic
Secret Service enrollment before opening its draft. If a dependency or API
change breaks those checks, the draft explicitly identifies failed/incomplete
validation and remains for repair and review. A failed dependency resolution can
leave Cargo.lock at the previous version; that draft is not buildable until
resolution and all checks pass. The workflow never automatically merges,
installs, or deploys a candidate. Candidate checks do not run native Omarchy/QML
or compare the installed plugin/helper pair; those remain release checks. It does not equate synthetic checks with
real-relay runtime compatibility.

GitHub token-created PRs can create approval-required `pull_request` runs for
opened/synchronize/reopened events. This workflow executes candidate checks
directly and links their run, without depending on that later approval. See
[GitHub’s current workflow-trigger behavior](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow). Repository Actions settings must allow token-created
PRs; otherwise creation fails visibly. The check job has read-only permissions;
candidate builds run with read-only repository permissions and no GH_TOKEN. A
separate publication job receives contents/PR write permissions, downloads only
this run’s candidate artifact, validates its three allowed paths and exact source
pin edits, and opens the draft without executing dependency code.
An existing PR for the same revision is retained instead of repeatedly creating
or overwriting it. Further fixes and reviews of that candidate are manual.

Local commands:

```sh
python3 scripts/check-upstream
python3 scripts/check-upstream --self-test
python3 scripts/check-upstream --upstream-revision <40-character-SHA> # offline comparison
python3 scripts/check-upstream --apply <40-character-SHA>             # explicit local candidate
cargo +1.95.0 update --manifest-path helper/Cargo.toml -p buzz-sdk -p buzz-ws-client
```

The apply command does not fetch dependencies, create a PR, or change runtime
installation. Inspect the candidate diff, resolve its lockfile, and run the
required checks before proposing it for merge. Review upstream signing,
authentication, membership, event projection, auxiliary completeness, and
resource-limit changes as compatibility work; keep the real-relay acceptance
route in RELAY_TESTING.md separate from local synthetic verification.

Updating the Omarchy plugin refreshes QML/plugin files only. Install the matching
helper build separately and restart its user service after a reviewed release.
Compare the installed plugin version with `omarchy-buzz --version`, which reports
helperVersion and backendRevision; a plugin update alone must not be reported as
a backend update. Both plugin and helper must be tested as one release pair.

A successful candidate’s lockfile must contain exactly one official `buzz-sdk`,
`buzz-ws-client`, and `buzz-core` package with both the git revision and resolved
commit equal to the candidate SHA. The read-only job captures the lockfile
SHA256 before tests and requires it unchanged afterward; the publication job
checks that digest against the uploaded artifact. These checks catch accidental
lockfile drift and mismatched revisions; they are not a sandbox proof against
malicious dependency code. Failed candidates remain explicitly failed.

If a previous publication pushed a candidate branch but could not create its PR,
a later run retains that orphan branch and stops with a clear job summary.
Inspect it and open a draft manually, or remove that owned candidate branch
before rerunning. The workflow never force-pushes or overwrites it.

The disposable real-relay runner additionally checks its inspected schema/startup
revision. A dependency-update candidate that changes Buzz must review and update
that runner baseline before running real conformance; source compilation alone
cannot certify changed relay behavior. The manual conformance workflow is not
automatically included in the daily dependency candidate's pass result.
