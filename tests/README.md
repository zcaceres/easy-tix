# Test coverage

Run `cargo test --release --locked --offline` (or `make test`).
The tests use only the Rust standard library: no test framework crates, Python,
Behave, registry downloads, or test-time network requests.

`common/mod.rs` creates an isolated directory per test, runs the actual executable
with bounded execution time, and drains output concurrently to avoid pipe-buffer
deadlocks. Tests do not change the process-wide environment or working directory,
so Cargo can run them in parallel. `TICKET_SCRIPT` optionally selects an absolute
path to the exact executable under test; otherwise Cargo supplies it.

## Retired BDD coverage

The 104 Gherkin scenarios were consolidated into ordinary Rust tests and tables
in `cli.rs`; the counts below describe the old suite, not the number of Rust test
functions. There is no replacement Gherkin parser or custom BDD framework.

| Former feature | Scenarios | Rust coverage |
| --- | ---: | --- |
| ID resolution | 10 | `id_resolution_exact_prefix_suffix_substring_and_ambiguity`, status/dependency/link partial-ID tests |
| Creation | 17 | `creation_defaults_and_all_options`, `invalid_creation_leaves_no_tickets` |
| Dependencies | 15 | `dependencies_are_resolved_idempotent_and_removable`, `tree_depth_then_id_order_full_output_and_cycles` |
| Directory selection | 10 | `directory_discovery_overrides_and_initialization` |
| Links | 7 | `links_are_symmetric_idempotent_and_resolved_before_writes` |
| Listings | 19 | `listing_filters_readiness_and_closed_blockers`, `ready_priority_sort_and_missing_dependencies`, `empty_listings_and_closed_paths_order_limits_and_full_search` |
| Notes | 7 | `notes_arguments_stdin_empty_and_existing_section` |
| Show | 10 | `show_content_and_all_relationship_sections`, ID-resolution and legacy-file tests |
| Status | 9 | `status_commands_and_partial_ids` |

## Former Python regression coverage

The same Rust suite retains the added regression checks for:

- No subprocess/runtime tool dependencies (empty PATH), plugin/Git/editor/pager
  non-execution, including an actual pseudo-terminal on Unix.
- Symlink rejection, invalid/path-traversal IDs, malformed/duplicate frontmatter,
  unknown options, and no partial writes on validation errors.
- Plain Markdown storage, literal special characters, unknown metadata retention,
  body/frontmatter separation, and the exact guide demonstration.
- Empty stores, closed-ticket mtime sorting, paths containing spaces/quotes/newlines
  (newlines on Unix), filtering before limiting, and scanning past 100 newer files.
- All editable fields, optional-field clearing, unchanged notes/unknown sections,
  fenced code, CRLF preservation, ambiguous Markdown, and parent-cycle rejection.
- Safe deletion with incoming references (including closed tickets), unsupported
  force flags, outgoing prerequisites, and ambiguous/missing IDs.

`installers.rs` executes the exact README installer code, not a reimplementation.
Shell HTTP mocks replace the old Python mocks; fixture hashes are known SHA-256
values. Tests cover platform selection, updates, corruption of the binary or
license, missing/duplicate checksums, HTTP errors, invalid tags, unsupported Unix
architectures, and destination-directory collisions.

`packaging.rs` exercises all five artifact names, complete asset-set validation,
tag/version matching, missing/empty assets, and checksum verification.

## Platform boundaries

- Common CLI cases run on all five release targets.
- The Bash guide demonstration, native PTY checks, and packaging-script regression
  tests run on Unix. Windows runs the same lifecycle coverage without Bash.
- Windows installer tests require `pwsh` and fail if it is missing; Unix installer
  tests require Bash and the checksum utilities used by the README snippets.
- Windows symlink tests print an explicit diagnostic if the account lacks symlink
  privilege; other errors fail the test.
- CI packages all five native binaries and verifies the combined release manifest.
  No production release is created by these tests.
