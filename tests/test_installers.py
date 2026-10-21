"""Execute the README installers with fake GitHub downloads, without networking."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


README = Path(__file__).resolve().parents[1] / "README.md"


def snippet(platform):
    text = README.read_text(encoding="utf-8")
    section = text.split(f"<!-- install-{platform}:start -->", 1)[1]
    return section.split("```", 2)[1].split("\n", 1)[1]


class InstallerFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ticket installer ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.downloads = self.root / "downloads"
        self.downloads.mkdir()
        self.destination = self.root / "install space"
        self.destination.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.env = os.environ.copy()
        self.env.update(
            TICKET_INSTALL_DIR=str(self.destination),
            MOCK_DOWNLOADS=str(self.downloads),
            MOCK_TAG="v12.34.56",
            MOCK_FAIL="",
            PATH=str(self.bin) + os.pathsep + self.env["PATH"],
        )

    def prepare(self, platform):
        self.asset = f"ticket-v12.34.56-{platform}"
        if platform.startswith("windows"):
            self.asset += ".exe"
        payloads = {self.asset: b"verified binary fixture\n", "LICENSE": b"license fixture\n"}
        for name, data in payloads.items():
            (self.downloads / name).write_bytes(data)
        self.manifest = "".join(
            f"{hashlib.sha256(data).hexdigest()}  {name}\n"
            for name, data in payloads.items()
        )
        (self.downloads / "SHA256SUMS").write_text(self.manifest)

    def verify_install(self, windows=False):
        name = "ticket.exe" if windows else "ticket"
        self.assertEqual(
            (self.destination / name).read_bytes(),
            (self.downloads / self.asset).read_bytes(),
        )
        self.assertEqual(
            (self.destination / "ticket.LICENSE").read_bytes(),
            (self.downloads / "LICENSE").read_bytes(),
        )
        self.assertEqual(list(self.destination.glob(".ticket*")), [])


@unittest.skipIf(os.name == "nt", "Unix installer; Windows installer tested separately")
class UnixInstallerTests(InstallerFixture):
    def setUp(self):
        super().setUp()
        self.env.update(MOCK_OS="Darwin", MOCK_CPU="arm64")
        mocks = {
            "uname": """
import os, sys
print(os.environ["MOCK_OS" if sys.argv[1] == "-s" else "MOCK_CPU"])
""",
            "gh": """
import os, pathlib, shutil, sys
a = sys.argv[1:]
if a[1] == os.environ.get("MOCK_FAIL"):
    sys.exit(1)
if a[:2] == ["release", "view"]:
    print(os.environ["MOCK_TAG"])
elif a[:2] == ["release", "download"]:
    assert a[2] == os.environ["MOCK_TAG"], "download must use the resolved tag"
    dest = pathlib.Path(a[a.index("--dir") + 1])
    for i, arg in enumerate(a):
        if arg == "--pattern":
            name = a[i+1]
            shutil.copyfile(pathlib.Path(os.environ["MOCK_DOWNLOADS"]) / name, dest / name)
else:
    sys.exit(2)
""",
        }
        for name, code in mocks.items():
            path = self.bin / name
            path.write_text(f"#!{sys.executable}\n{code}")
            path.chmod(0o755)
        self.prepare("macos-arm64")

    def run_installer(self, ok=True):
        result = subprocess.run(
            ["bash", "-c", snippet("unix")], env=self.env, cwd=self.root,
            text=True, capture_output=True, timeout=20,
        )
        self.assertEqual(result.returncode == 0, ok, result.stdout + result.stderr)
        return result

    def test_platform_selection_and_update(self):
        for os_name, cpu, platform in (
            ("Darwin", "arm64", "macos-arm64"),
            ("Darwin", "x86_64", "macos-x86_64"),
            ("Linux", "aarch64", "linux-arm64"),
            ("Linux", "x86_64", "linux-x86_64"),
        ):
            with self.subTest(platform=platform):
                self.env.update(MOCK_OS=os_name, MOCK_CPU=cpu)
                self.prepare(platform)
                (self.destination / "ticket").write_bytes(b"old binary")
                self.run_installer()
                self.verify_install()
                self.assertTrue(os.access(self.destination / "ticket", os.X_OK))

    def test_bad_checksum_preserves_existing_binary(self):
        for name in (self.asset, "LICENSE"):
            with self.subTest(name=name):
                self.prepare("macos-arm64")
                (self.destination / "ticket").write_bytes(b"old binary")
                (self.downloads / name).write_bytes(b"corrupt download")
                self.run_installer(ok=False)
                self.assertEqual((self.destination / "ticket").read_bytes(), b"old binary")

    def test_missing_and_duplicate_checksums(self):
        for manifest in ("", self.manifest + self.manifest, self.manifest.splitlines()[1] + "\n"):
            with self.subTest(manifest=manifest):
                (self.downloads / "SHA256SUMS").write_text(manifest)
                self.run_installer(ok=False)
                self.assertFalse((self.destination / "ticket").exists())

    def test_github_errors_and_unsupported_platform(self):
        for step in ("view", "download"):
            self.env["MOCK_FAIL"] = step
            self.run_installer(ok=False)
            self.assertFalse((self.destination / "ticket").exists())
        self.env.update(MOCK_FAIL="", MOCK_CPU="riscv64")
        self.assertIn("Unsupported system", self.run_installer(ok=False).stderr)

    def test_unexpected_tag_and_directory_collision(self):
        self.env["MOCK_TAG"] = "../invalid"
        self.run_installer(ok=False)
        self.env["MOCK_TAG"] = "v12.34.56"
        (self.destination / "ticket").mkdir()
        self.run_installer(ok=False)


@unittest.skipUnless(os.name == "nt" and shutil.which("pwsh"), "Requires Windows PowerShell")
class WindowsInstallerTests(InstallerFixture):
    def run_installer(self, ok=True):
        mock = r"""
function gh {
    $a = @($args)
    $global:LASTEXITCODE = 0
    if ($a[1] -eq $env:MOCK_FAIL) { $global:LASTEXITCODE = 1; return }
    if ($a[1] -eq 'view') { return $env:MOCK_TAG }
    if ($a[1] -ne 'download' -or $a[2] -ne $env:MOCK_TAG) { throw 'Wrong download tag' }
    $dest = $a[[Array]::IndexOf($a, '--dir') + 1]
    for ($i = 0; $i -lt $a.Count; $i++) {
        if ($a[$i] -eq '--pattern') {
            Copy-Item -LiteralPath (Join-Path $env:MOCK_DOWNLOADS $a[$i+1]) -Destination $dest
        }
    }
}
"""
        script = self.root / "install-test.ps1"
        script.write_text(mock + "\n" + snippet("windows"), encoding="utf-8")
        result = subprocess.run(
            ["pwsh", "-NoProfile", "-NonInteractive", "-File", str(script)],
            cwd=self.root, env=self.env, capture_output=True, text=True, timeout=20,
        )
        self.assertEqual(result.returncode == 0, ok, result.stdout + result.stderr)

    def test_install_and_update(self):
        self.prepare("windows-x86_64")
        (self.destination / "ticket.exe").write_bytes(b"old binary")
        self.run_installer()
        self.verify_install(windows=True)

    def test_bad_checksum_and_download_failure(self):
        self.prepare("windows-x86_64")
        (self.destination / "ticket.exe").write_bytes(b"old binary")
        (self.downloads / self.asset).write_bytes(b"corrupt")
        self.run_installer(ok=False)
        self.assertEqual((self.destination / "ticket.exe").read_bytes(), b"old binary")
        self.env["MOCK_FAIL"] = "download"
        self.run_installer(ok=False)
        self.assertEqual((self.destination / "ticket.exe").read_bytes(), b"old binary")


if __name__ == "__main__":
    unittest.main()
