# Contributing to Resona

Resona welcomes focused fixes, source-format fixtures, and UI improvements. Start with an issue for substantial product or storage changes.

## Development

Use the pinned Rust/Node/pnpm versions. macOS desktop builds require Xcode Command Line Tools. No production signing credentials are needed.

```bash
pnpm install --frozen-lockfile
pnpm dev:web          # synthetic UI data
pnpm dev              # real local collection on macOS
pnpm check
pnpm check:desktop    # macOS
```

## Boundaries

- `crates/resona-core`: allowlist parsing, identity, reduction, SQLite, and query semantics. No Tauri/WebView dependency.
- `src-tauri`: native window lifecycle, menu bar, filesystem notifications, and narrow IPC commands.
- `src`: presentation and interaction. Do not recalculate trusted backend metrics in React.
- Source logs are read-only. Production storage is fixed under the user's home. Tests inject temporary paths and must not overwrite HOME or read a contributor's sessions.
- Preserve unknown values as `null` / `N/A`. Never infer thread identity from the last UUID in a reverted filename.

## Style and tests

Use rustfmt and Clippy (`-D warnings`), strict TypeScript, ESLint, and Prettier. Prefer typed errors and explicit dependencies; explain narrowly scoped lint exceptions. Use English identifiers/comments and concise Conventional Commit titles, following `feat`, `fix`, `perf`, `test`, `docs`, or `ci`.

Tests should describe observable behavior: deduplication, lineage, counter resets, transaction recovery, query boundaries, and meaningful interactions. Use small synthetic fixtures. Do not attach raw conversation logs to public issues or commit real databases.

PR descriptions contain **Summary** and **Test plan**. State unrun checks honestly, update Changelog for user-facing changes, and explain migrations or metric changes. Commit lockfile updates with dependency changes. Generated types and documented SQL must remain consistent.

## Releases

Maintainers use the [release procedure](docs/releasing.md). A normal contribution must not publish packages, move tags, or modify signing credentials.

## Dependency updates

Dependabot groups minor and patch version updates by ecosystem. Major version upgrades are reviewed deliberately with compatibility checks and are not opened automatically. Security auditing remains part of pull-request CI and the weekly dependency workflow.
