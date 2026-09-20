# Resona engineering

- Read `docs/technical-design.md` and preserve the approved product behavior.
- `resona-core` has no Tauri dependency. Native integration belongs in `src-tauri`, presentation in `src`.
- Source logs are read-only; production data stays in `~/.resona`. Tests inject temporary roots.
- Keep null metrics unknown, and preserve thread/rollout/physical-file/turn identities independently.
- Use rustfmt, Clippy with warnings denied, strict TypeScript, ESLint and Prettier. Run relevant checks before handoff.
- Never commit real session content, user databases, local diagnostics, or signing materials.
- Use focused Conventional Commit messages and PR sections Summary / Test plan.
- Do not edit code on main/master. Use grove for additional worktrees.
- Do not publish, move tags or overwrite release assets without task authorization.
