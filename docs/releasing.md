# Release procedure

1. Prepare a version PR: use `node scripts/version.mjs --sync VERSION`, refresh Cargo.lock, write the matching CHANGELOG section, run all required checks, and verify data compatibility.
2. Merge the tested commit into the default branch. Tag that exact commit `vVERSION` and push the tag. Do not move an existing release tag.
3. `Release` reuses CI, builds both macOS architectures, signs and notarizes them, then creates a **draft** with DMGs, SHA256SUMS, a release manifest, and notes. Missing Apple credentials fail the release job; there is no silent unsigned fallback.
4. Download the exact draft artifacts. Verify SHA256, Gatekeeper, installation, tray behavior, source collection, upgrade/backup behavior, and the documented macOS/architecture matrix. Publish the draft only after verification. Mark release candidates as prereleases.
5. A failed build can be rerun for the same tag/SHA. Changed binaries require new checksums and revalidation. Never replace already public version assets with a different build; issue a new patch version.

## Publishing configuration

The GitHub repository needs `APPLE_CERTIFICATE` (base64 p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` (Developer ID Application), `APPLE_API_ISSUER`, `APPLE_API_KEY`, and `APPLE_API_PRIVATE_KEY` (p8 content). Only the release signing jobs receive them. Ordinary fork PRs need none of these.

The bundle identifier is `io.github.xuzhu-591.resona`. Keep it stable after the first release. The installer supports macOS 13+, with separate Apple Silicon and Intel artifacts. CI on a newer macOS does not substitute for a minimum-OS installation test.

## Recovery

For a defective release, stop recommending it as latest, document the problem, and publish a fix. App downgrade must honor SQLite schema compatibility. Quit the writer before restoring a consistent backup; do not mix a database with unrelated WAL/SHM files. Resona source logs and the legacy plugin database remain unchanged.
