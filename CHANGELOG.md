# Changelog

## [Unreleased]

## [0.1.0]

### Features

- Standalone macOS menu bar app with response trends, TTFT and speed distributions, model summaries, turn details, and settings.
- Local Codex active/archive and Claude Code collection with multi-rollout identity, inherited-prefix handling, token deduplication, and missing-evidence states.
- Fixed local SQLite storage, read-only legacy import, and isolated parser/database fixtures.

### Fixes

- Restore left-click popover interaction and separate the right-click menu.
- Anchor the popover below the menu bar using consistent physical screen coordinates, with no added top gap.
- Restore prototype-aligned settings tabs, metric controls, distribution labels, and recent activity.
- Add complete English and Simplified Chinese documentation with language switching.

### Data Compatibility

- New database at `~/.resona/monitor.sqlite3`; existing source logs and the old plugin database are not modified.
- Legacy metrics remain unverified until reparsed. Corrected identity and token counting can change old totals.
