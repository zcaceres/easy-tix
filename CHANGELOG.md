# Changelog

## 0.1.0 — standalone Rust binaries

- Replace Bash with a dependency-free Rust executable; retain Markdown storage
  and the local-only command workflow.
- Embed the agent guide (`ticket agent-guide`) and add `--version`.
- Validate malformed frontmatter and CLI arguments; consistently send errors to
  stderr. Preserve unknown metadata and Markdown when updating tickets.
- Publish five native-tested release binaries with SHA-256 checksums through
  GitHub Actions: macOS Intel/ARM64, Linux x86-64/ARM64, Windows x86-64.

## Unreleased — local-only foundation

- Start a fresh repository while retaining original MIT attribution.
- Remove plugin discovery/dispatch, editor and pager launching, Git assignee
  lookup, migration tools, packaging, and publishing integrations.
- Bring list/filter functionality into the core CLI.
- Validate ticket IDs, creation metadata, priority, and type.
- Reject symlinked/non-regular ticket entries; use unpredictable temporary files
  and prevent creation from overwriting an existing ticket.
- Treat metadata updates as data rather than generated sed code.
- Add offline standard-library regression tests.
