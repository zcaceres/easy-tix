# Project guidance

See README.md for behavior and the local-only security boundary.
For using Tickets to manage tasks, read AGENT_GUIDE.md and run `./ticket help`.

- Keep runtime behavior local. Do not add plugins, shell evaluation, editor or
  pager execution, or network integrations to the CLI.
- GitHub Actions may build and publish the five approved release binaries.
- The CLI is a standalone Rust executable `ticket`, with Markdown ticket storage.
- Treat ticket input as data, never executable code.
- Run `make test` for offline Rust unit and CLI integration tests.
- Installer tests run the README snippets in Bash (Unix) or PowerShell (Windows).
- Update README.md and tests when changing command behavior.
- Retain LICENSE and attribution when reusing the original implementation.
