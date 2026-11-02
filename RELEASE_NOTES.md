# Tickets v0.2.1

- Scalar metadata now preserves leading/trailing whitespace across reads,
  assignee filtering, and unrelated updates. Unknown metadata is preserved too.
- `dep tree` now uses iterative strongly connected component traversal and
  reverse-topological depth calculation instead of enumerating shared paths.
  The 48-ticket shared-dependency stress case fell from a 3-second timeout to
  about 5 ms on the tested Apple Silicon Mac; timings are machine-specific.
- Agent instructions and regular examples now use `ticket` on PATH by default.
  Project-local executables remain supported as an explicit alternative.
- Regression coverage includes shared graphs, cycles, depth boundaries,
  metadata whitespace, and the exact PATH-based embedded agent demonstration.

Download the executable for your OS and CPU. No Rust toolchain, Bash, Python,
package manager, or external command-line utilities are required to run it.
The repository and release downloads are public; no GitHub account is required.
The README includes platform-detecting installers with checksum verification.

| Platform | Asset |
| --- | --- |
| macOS Apple Silicon | `ticket-v0.2.1-macos-arm64` |
| macOS Intel | `ticket-v0.2.1-macos-x86_64` |
| Linux x86-64 | `ticket-v0.2.1-linux-x86_64` |
| Linux ARM64 | `ticket-v0.2.1-linux-arm64` |
| Windows x86-64 | `ticket-v0.2.1-windows-x86_64.exe` |

Rename the download to `ticket` (`ticket.exe` on Windows). On macOS/Linux,
run `chmod +x ticket`, then place it on PATH and run `ticket help` and
`ticket agent-guide`. The README installer defaults to `~/.local/bin`.
For an optional project-local executable, use `./ticket help` on macOS/Linux
or `.\ticket.exe help` in PowerShell instead.

Verify the download against `SHA256SUMS` (SHA-256). The checksums detect changed
downloads but are not an independent signing identity. macOS and Windows builds
are not notarized/Developer-ID or Authenticode signed, so the OS may display an
unidentified-developer warning. Verify the source and checksum before approving
this specific executable; do not disable system-wide security protections.

Linux executables use musl static linking. Windows includes the C runtime
statically. macOS uses the system libraries supplied with macOS.

Existing `.tickets/*.md` files require no migration. Commands, relationships,
tags, notes, filtering, and directory discovery are retained. Input validation
is stricter: malformed/duplicate frontmatter, mismatched IDs, and unknown CLI
options fail explicitly. Unknown metadata fields and Markdown bodies survive
updates. Dependency graph traversals are limited to 256 levels.

No runtime network access, subprocess execution, plugins, editors, pagers,
telemetry, or Git synchronization. Coordinate writers; status changes are not
locks and multi-ticket writes are not transactions.
