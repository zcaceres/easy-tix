"""Package pre-tested executables; never build or download dependencies here."""

import hashlib
import os
from pathlib import Path
import shutil
import sys
import tomllib


version = tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]
tag = os.environ.get("GITHUB_REF_NAME", "")
if os.environ.get("GITHUB_REF_TYPE") == "tag" and tag != f"v{version}":
    raise SystemExit(f"Tag {tag!r} does not match Cargo version v{version}")

dist = Path("dist")
dist.mkdir(exist_ok=True)
platforms = ("macos-arm64", "macos-x86_64", "linux-x86_64", "linux-arm64", "windows-x86_64")


def asset_name(platform):
    return f"ticket-v{version}-{platform}" + (".exe" if platform.startswith("windows") else "")


if sys.argv[1:] == ["--checksums"]:
    expected = {asset_name(platform) for platform in platforms}
    actual = {path.name for path in dist.iterdir()}
    if expected != actual:
        raise SystemExit(f"Asset mismatch: expected {sorted(expected)}, got {sorted(actual)}")
    for name in ("LICENSE", "AGENT_GUIDE.md"):
        shutil.copy2(name, dist / name)
    lines = []
    for path in sorted(dist.iterdir()):
        if path.stat().st_size == 0:
            raise SystemExit(f"Empty release asset: {path}")
        lines.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
    (dist / "SHA256SUMS").write_text("".join(lines))
elif not sys.argv[1:]:
    platform = os.environ["ASSET"]
    if platform not in platforms:
        raise SystemExit(f"Unknown platform {platform}")
    filename = "ticket.exe" if platform.startswith("windows") else "ticket"
    source = Path("target") / os.environ["TARGET"] / "release" / filename
    shutil.copy2(source, dist / asset_name(platform))
else:
    raise SystemExit("Usage: package.py [--checksums]")
