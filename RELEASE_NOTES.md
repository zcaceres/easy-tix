# Tickets v0.2.0

- `update` edits title, metadata, description, design, and acceptance criteria
  while preserving notes and unrelated content.
- `delete` removes an unreferenced ticket without a prompt; incoming dependencies,
  links, or parent references must be resolved explicitly.
- Tests now run entirely through Cargo, with Bash/PowerShell installer checks;
  Python and Behave are no longer development or release requirements.
- Release automation uses SHA-pinned Node.js 24 actions.

Download the executable for your OS and CPU. No Rust toolchain, Bash, Python,
package manager, or external command-line utilities are required to run it.
The repository and release downloads are public; no GitHub account is required.
The README includes platform-detecting installers with checksum verification.

| Platform | Asset |
| --- | --- |
| macOS Apple Silicon | `ticket-v0.2.0-macos-arm64` |
| macOS Intel | `ticket-v0.2.0-macos-x86_64` |
| Linux x86-64 | `ticket-v0.2.0-linux-x86_64` |
| Linux ARM64 | `ticket-v0.2.0-linux-arm64` |
| Windows x86-64 | `ticket-v0.2.0-windows-x86_64.exe` |

Rename the download to `ticket` (`ticket.exe` on Windows). On macOS/Linux,
run `chmod +x ticket`, then `./ticket help`. On Windows PowerShell, use
`.\ticket.exe help`. Run `ticket agent-guide` for embedded agent instructions.

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
