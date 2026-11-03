# Restricted review evidence

The current qualification is the reviewed development application and its separate local daily candidate. The top-level receipt.json indexes both, their immutable binary artifacts, mutation controls, native checks, report QA and source manifests. Historical evidence is under history-initial and is not current qualification.

Development source is99b9c904; its full application tree and explicit file fingerprint are recorded. The local daily candidate combines that source with existing palette/note previews; daily-source.json names its parents and its three-file application delta. It has its own full suite, lint, OS, optimized-build and native receipts. It is local only and not installed.

The source fingerprint covers the exact application src/tests, Cargo manifests, CI workflow and security driver files listed in each receipt. environment.json separately records the committed application trees (including assets/fixtures), GPUI revision/modified-file hashes, patch files, native helpers and runtime versions. This is evidence of these executions, not a byte-reproducible-build or complete supply-chain attestation.

Each full-suite summary is derived from its raw executed log. receipt.json inside the qualification directory hashes the raw logs, which remain under the local operational archive named by the top-level receipt. The repository retains concise full-suite summaries, full OS/lint/build logs, and individual mutation failure/baseline/restoration logs. Source and log hashes are never reassigned to a different run. A qualification receipt's binary field is the compiler output path at recording; use the top-level immutable artifact path and digest for later binary verification.

The 21 mutations remove one guard at a time. Module controls use the actual module/filter with a small host-pane stub; real host attribution is covered by the separate native application tests. Application routing and startup mutations use actual scratch windows. Compile failures do not count. An earlier invalid kernel mutation and invalid injection fixtures remain in local historical operational evidence; the final controls were rerun after correction.

The public and local report have identical main reading content. The local report also preserves the reader's existing annotation. That private annotation is excluded from this repository, from the public HTML and from its notes JSON.

Portable launch and qualification commands are in scripts/security/README.md. Use a short TMPDIR and a private Cargo target as described there. report-qa.mjs verifies responsive layout, self-contained figures and add/export/embed/reopen annotations with the Chromium sandbox enabled. install-candidate.py defaults to inspection; --apply selects checked immutable bytes for future launches only, with an expected-launcher guard and a shared deployment lock. Its tests use temporary paths. This review never invokes --apply against the real launcher.

Raw log formatting is preserved byte for byte. Directory-scoped attributes exempt .log/.stderr evidence from whitespace checks; source and generator files remain checked.
