# Changelog

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
