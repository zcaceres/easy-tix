"""Offline CLI integration tests; no third-party dependencies."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path(os.environ.get("TICKET_SCRIPT", ROOT / "target" / "release" /
                            ("ticket.exe" if os.name == "nt" else "ticket"))).resolve()


class TicketTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ticket_test_")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.store = self.root / ".tickets"
        self.env = os.environ.copy()
        for key in ("TICKETS_DIR", "BASH_ENV", "ENV", "SHELLOPTS", "BASHOPTS"):
            self.env.pop(key, None)
        self.env["TICKETS_DIR"] = str(self.store)

    def cli(self, *args, ok=True, input=None, cwd=None):
        result = subprocess.run(
            [str(SCRIPT), *args], cwd=cwd or self.root, env=self.env,
            input=input, text=True, encoding="utf-8", capture_output=True, timeout=10,
        )
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result.stdout

    def create(self, title="Example", *args):
        ticket_id = self.cli("create", title, *args).strip()
        self.assertRegex(ticket_id, r"^[a-z0-9]+-[a-z0-9]{4}$")
        return ticket_id

    def test_lifecycle_and_plain_text(self):
        tid = self.create("First", "-d", "Description", "--tags", "ui,backend")
        self.assertIn("# First", (self.store / f"{tid}.md").read_text())
        for command, status in (("start", "in_progress"), ("close", "closed"),
                                ("reopen", "open")):
            self.cli(command, tid)
            self.assertIn(f"status: {status}", self.cli("show", tid))
        self.cli("add-note", tid, input="A note from stdin\n")
        self.assertIn("A note from stdin", self.cli("show", tid))

    def test_relationships(self):
        parent = self.create("Parent")
        child = self.create("Child", "--parent", parent)
        self.cli("dep", child, parent)
        self.assertIn(child, self.cli("blocked"))
        self.assertNotIn(child, self.cli("ready"))
        self.assertIn(child, self.cli("show", parent))
        self.cli("close", parent)
        self.assertIn(child, self.cli("ready"))
        self.cli("undep", child, parent)
        self.cli("link", parent, child)
        self.assertIn(f"links: [{child}]", self.cli("show", parent))
        self.cli("unlink", parent, child)
        self.assertIn("links: []", self.cli("show", parent))
        self.cli("dep", child, parent)
        self.assertIn(parent, self.cli("dep", "tree", child))
        self.cli("dep", "cycle")

    def test_listing_filters(self):
        one = self.create("One", "-a", "Alice", "--tags", "ui,backend")
        two = self.create("Two", "-a", "Bob", "--tags", "api")
        self.assertIn(one, self.cli("list", "-T", "ui"))
        self.assertNotIn(two, self.cli("ls", "-a", "Alice"))
        self.cli("close", two)
        self.assertIn(two, self.cli("ls", "--status=closed"))
        self.assertNotIn(one, self.cli("ls", "--status=closed"))
        self.assertIn(two, self.cli("closed", "--limit=1"))

    def test_partial_and_invalid_ids(self):
        tid = self.create()
        self.assertIn(tid, self.cli("show", tid[-4:]))
        for value in ("../outside", "*", "", "/tmp/example"):
            with self.subTest(value=value):
                self.cli("show", value, ok=False)
                self.cli("add-note", value, "bad", ok=False)

    def test_invalid_creation_metadata(self):
        for args in (("-p", "9"), ("-t", "unknown"), ("--tags", "ui\nstatus: closed"),
                     ("-a", "Alice\rBob"), ("--parent", "../outside"), ("-d",)):
            with self.subTest(args=args):
                self.cli("create", "Bad", *args, ok=False)
        self.assertEqual(list(self.store.glob("*.md")), [])

    def test_plugins_git_and_launchers_are_not_called(self):
        bindir = self.root / "bin"
        bindir.mkdir()
        marker = self.root / "executed"
        for name in ("tk-create", "ticket-create", "tk-unknown", "ticket-unknown",
                     "tk-help", "git", "launcher"):
            path = bindir / name
            path.write_text(f"#!/bin/sh\nprintf executed > '{marker}'\nexit 42\n")
            path.chmod(0o755)
        self.env["PATH"] = str(bindir) + os.pathsep + self.env["PATH"]
        for key in ("EDITOR", "PAGER", "TICKET_PAGER"):
            self.env[key] = str(bindir / "launcher")
        tid = self.create()
        self.cli("help")
        self.cli("show", tid)
        for cmd in ("unknown", "edit", "query", "migrate-beads", "super"):
            self.cli(cmd, tid, ok=False)
        self.assertFalse(marker.exists())

        # The old pager path only ran when stdout was a terminal.
        if os.name != "nt":
            import pty
            master, slave = pty.openpty()
            try:
                result = subprocess.run(
                    [str(SCRIPT), "show", tid], cwd=self.root, env=self.env,
                    stdin=subprocess.DEVNULL, stdout=slave, stderr=subprocess.PIPE,
                    timeout=10,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse(marker.exists())
            finally:
                os.close(slave)
                os.close(master)

    def test_symlink_entries_rejected(self):
        tid = self.create()
        outside = self.root / "outside.md"
        outside.write_text("untouched")
        try:
            (self.store / "escape.md").symlink_to(outside)
        except OSError:
            if os.name == "nt":
                self.skipTest("Windows account lacks symlink permission")
            raise
        for args in (("show", "escape"), ("add-note", "escape", "bad"),
                     ("ls",), ("ready",), ("close", tid)):
            self.cli(*args, ok=False)
        self.assertEqual(outside.read_text(), "untouched")

    def test_body_is_not_updated_as_metadata(self):
        tid = self.create("Body", "-d", "---\nstatus: keep this\n---")
        self.cli("close", tid)
        content = (self.store / f"{tid}.md").read_text()
        self.assertIn("status: closed", content)
        self.assertIn("status: keep this", content)

    def test_parent_directory_discovery(self):
        tid = self.create()
        del self.env["TICKETS_DIR"]
        nested = self.root / "src" / "deep"
        nested.mkdir(parents=True)
        self.assertIn(tid, self.cli("ls", cwd=nested))

    def test_empty_list(self):
        self.store.mkdir()
        for command in ("ls", "ready", "blocked", "closed"):
            with self.subTest(command=command):
                self.assertEqual(self.cli(command), "")

    def test_binary_runs_without_external_programs(self):
        self.env["PATH"] = ""
        self.env["BASH_ENV"] = str(self.root / "must-not-be-read")
        self.assertIn("ticket 0.1.0", self.cli("--version"))
        self.assertEqual(self.cli("agent-guide"), (ROOT / "AGENT_GUIDE.md").read_text(encoding="utf-8"))
        parent = self.create("Parent")
        child = self.create("Child", "--parent", parent)
        self.cli("dep", child, parent)
        self.cli("start", parent)
        self.cli("add-note", parent, input="No external processes needed\n")
        self.cli("close", parent)
        self.assertIn(child, self.cli("ready"))
        self.assertIn(parent, self.cli("closed"))
        self.assertIn(parent, self.cli("dep", "tree", child))

    def test_legacy_file_round_trip(self):
        self.store.mkdir()
        path = self.store / "legacy-0001.md"
        body = "# Literal | title : ✓\n\n---\nstatus: leave this alone\n$(touch injected)\n"
        path.write_text(
            "---\nid: legacy-0001\nstatus: open\ndeps: []\nlinks: []\n"
            "created: 2024-01-01T00:00:00Z\ntype: task\npriority: 1\n"
            "custom: keep / & \\\nassignee: A: B\ntags: [ui, api]\n---\n" + body,
            encoding="utf-8",
        )
        self.assertIn("Literal | title : ✓", self.cli("ready", "-a", "A: B"))
        self.cli("start", "0001")
        content = path.read_text(encoding="utf-8")
        self.assertTrue(content.endswith(body))
        self.assertIn("custom: keep / & \\\n", content)
        self.assertFalse((self.root / "injected").exists())

    def test_bad_options_and_malformed_files_fail(self):
        tid = self.create()
        for args in (("ready", "--unknown"), ("blocked", "-a"), ("show", tid, "extra"),
                     ("status", tid), ("create", "--tags", "[bad]")):
            with self.subTest(args=args):
                self.cli(*args, ok=False)
        path = self.store / f"{tid}.md"
        original = path.read_text()
        path.write_text(original.replace("status: open", "status: open\nstatus: closed"))
        self.cli("show", tid, ok=False)
        self.cli("close", tid, ok=False)
        self.assertIn("status: open\nstatus: closed", path.read_text())

    def test_cycles_missing_dependencies_and_idempotent_links(self):
        one = self.create("One")
        two = self.create("Two")
        self.cli("dep", one, two)
        self.cli("dep", two, one)
        self.assertIn("Dependency cycles:", self.cli("dep", "cycle"))
        self.cli("dep", "tree", "--full", one)
        self.cli("undep", two, one)
        self.assertIn("No dependency cycles", self.cli("dep", "cycle"))
        self.cli("link", one, two)
        path = self.store / f"{one}.md"
        before = path.stat().st_mtime_ns
        self.assertEqual(self.cli("link", one, two).strip(), "All links already exist")
        self.assertEqual(before, path.stat().st_mtime_ns)
        self.cli("unlink", one, two)
        self.cli("unlink", one, two, ok=False)
        (self.store / f"{two}.md").unlink()
        self.assertIn(one, self.cli("blocked"))
        self.assertNotIn(one, self.cli("ready"))
        self.assertIn("[missing]", self.cli("show", one))

    def test_failed_multi_link_does_not_partially_change_files(self):
        one = self.create("One")
        two = self.create("Two")
        self.cli("link", one, two, "nonexistent", ok=False)
        self.assertIn("links: []", self.cli("show", one))
        self.assertIn("links: []", self.cli("show", two))

    def test_closed_paths_order_filters_and_limit(self):
        dirname = "project space's store" if os.name == "nt" else "project space's\nstore"
        self.store = self.root / dirname / ".tickets"
        self.env["TICKETS_DIR"] = str(self.store)
        older = self.create("Older", "--tags", "old")
        newer = self.create("Newer", "--tags", "new")
        self.cli("close", older)
        self.cli("close", newer)
        os.utime(self.store / f"{older}.md", (100, 100))
        os.utime(self.store / f"{newer}.md", (200, 200))
        output = self.cli("closed")
        self.assertEqual([line.split()[0] for line in output.splitlines()], [newer, older])
        self.assertIn(newer, self.cli("closed", "--limit=1"))
        self.assertNotIn(older, self.cli("closed", "--limit=1"))
        self.assertIn(older, self.cli("closed", "-T", "old", "--limit=1"))
        self.assertEqual(self.cli("closed", "--limit=0"), "")
        self.cli("closed", "--limit=bad", ok=False)
        self.cli("closed", "--limit=-1", ok=False)

    def test_closed_searches_past_newer_open_tickets(self):
        tid = self.create("Older closed ticket")
        self.cli("close", tid)
        path = self.store / f"{tid}.md"
        content = path.read_text()
        os.utime(path, (100, 100))
        for i in range(101):
            other = f"open-{i:04d}"
            (self.store / f"{other}.md").write_text(
                content.replace(tid, other).replace("status: closed", "status: open")
            )
        self.assertIn(tid, self.cli("closed", "--limit=1"))

    @unittest.skipIf(os.name == "nt", "Bash example; native lifecycle tested separately")
    def test_documented_agent_workflow(self):
        guide = (ROOT / "AGENT_GUIDE.md").read_text()
        demo = guide.split("<!-- agent-demo:start -->", 1)[1]
        demo = demo.split("<!-- agent-demo:end -->", 1)[0]
        script = demo.split("```bash\n", 1)[1].split("```", 1)[0]
        shutil.copy2(SCRIPT, self.root / "ticket")
        env = self.env.copy()
        env.pop("TICKETS_DIR")
        result = subprocess.run(
            ["bash", "-c", script], cwd=self.root, env=env,
            text=True, capture_output=True, timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        files = list(self.store.glob("*.md"))
        self.assertEqual(len(files), 2)
        contents = [path.read_text() for path in files]
        for content in contents:
            self.assertIn("status: closed\n", content)
            self.assertIn("## Notes", content)
            self.assertIn("Demo only:", content)
        prerequisite = next(p for p in files if "# Demo: define interface" in p.read_text())
        delivery = next(p for p in files if "# Demo: implement interface" in p.read_text())
        self.assertIn(f"deps: [{prerequisite.stem}]", delivery.read_text())
        self.assertIn(f"{prerequisite.stem} [P2][open]", result.stdout)
        self.assertIn(f"{delivery.stem} [P2][open]", result.stdout)
        self.assertIn(f"<- [{prerequisite.stem}]", result.stdout)


if __name__ == "__main__":
    unittest.main()
