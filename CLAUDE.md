# CLAUDE.md

@AGENTS.md

## Claude Code notes

- `AGENTS.md` (imported above) is the single source of truth for project rules; keep agent guidance there, not here.
- Shared project settings live in `.claude/settings.json`; personal overrides go in `.claude/settings.local.json` (git-ignored).
- When reverse engineering locally with an MCP or decompiler, keep all vendor files under the git-ignored `songpal_extracted/` and never paste decompiled code into the repository.
