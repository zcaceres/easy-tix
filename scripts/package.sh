#!/usr/bin/env bash
# Package pre-tested executables; never build or download dependencies here.
set -euo pipefail
version=$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' Cargo.toml)
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Invalid package version" >&2; exit 1; }
if [[ "${GITHUB_REF_TYPE:-}" == tag && "${GITHUB_REF_NAME:-}" != "v$version" ]]; then
    echo "Tag does not match Cargo version v$version" >&2
    exit 1
fi
mkdir -p dist
platforms=(macos-arm64 macos-x86_64 linux-x86_64 linux-arm64 windows-x86_64)
asset_name() {
    local suffix=
    [[ "$1" != windows-* ]] || suffix=.exe
    printf 'ticket-v%s-%s%s' "$version" "$1" "$suffix"
}
if [[ "${1:-}" == --checksums && $# == 1 ]]; then
    shopt -s nullglob dotglob
    files=(dist/*)
    [[ ${#files[@]} == 5 ]] || { echo "Expected exactly five release binaries" >&2; exit 1; }
    for platform in "${platforms[@]}"; do
        file="dist/$(asset_name "$platform")"
        [[ -f "$file" && -s "$file" && ! -L "$file" ]] || { echo "Missing or empty asset: $file" >&2; exit 1; }
    done
    cp LICENSE AGENT_GUIDE.md dist/
    names=(AGENT_GUIDE.md LICENSE)
    for platform in "${platforms[@]}"; do names+=("$(asset_name "$platform")"); done
    (
        cd dist
        if command -v sha256sum >/dev/null; then sha256sum "${names[@]}"
        else shasum -a 256 "${names[@]}"
        fi
    ) > dist/SHA256SUMS
elif [[ $# == 0 ]]; then
    case "${ASSET:-}/${TARGET:-}" in
        macos-arm64/aarch64-apple-darwin|macos-x86_64/x86_64-apple-darwin|\
        linux-arm64/aarch64-unknown-linux-musl|linux-x86_64/x86_64-unknown-linux-musl|\
        windows-x86_64/x86_64-pc-windows-msvc) ;;
        *) echo "Unknown platform/target combination" >&2; exit 1 ;;
    esac
    binary=ticket
    [[ "$ASSET" != windows-* ]] || binary=ticket.exe
    source="target/$TARGET/release/$binary"
    [[ -s "$source" ]] || { echo "Missing or empty executable: $source" >&2; exit 1; }
    cp "$source" "dist/$(asset_name "$ASSET")"
else
    echo "Usage: package.sh [--checksums]" >&2
    exit 1
fi
