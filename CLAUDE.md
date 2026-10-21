# Project guidance

See README.md for behavior and the local-only security boundary.
For using Tickets to manage tasks, read AGENT_GUIDE.md and run `./ticket help`.

- Keep runtime behavior local. Do not add plugins, shell evaluation, editor or
  pager execution, network integrations, or publishing automation.
- The CLI is the single Bash executable `ticket`, with Markdown ticket storage.
- Treat ticket input as data, not shell/sed/awk source.
- Run `make test` for offline standard-library regression tests.
- `make test-bdd` is optional and requires an already-installed Behave.
- Update README.md and tests when changing command behavior.
- Retain LICENSE and attribution when reusing the original implementation.
