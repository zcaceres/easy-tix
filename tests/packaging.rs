#![cfg(unix)]
mod common;
use common::*;
use std::fs;
use std::process::Command;

const PLATFORMS: [(&str, &str); 5] = [
    ("macos-arm64", "aarch64-apple-darwin"),
    ("macos-x86_64", "x86_64-apple-darwin"),
    ("linux-x86_64", "x86_64-unknown-linux-musl"),
    ("linux-arm64", "aarch64-unknown-linux-musl"),
    ("windows-x86_64", "x86_64-pc-windows-msvc"),
];

fn command(s: &Sandbox) -> Command {
    let mut c = Command::new("bash");
    c.arg(project().join("scripts/package.sh"))
        .current_dir(&s.root)
        .env_remove("GITHUB_REF_TYPE")
        .env_remove("GITHUB_REF_NAME");
    c
}

fn setup() -> Sandbox {
    let s = Sandbox::new();
    fs::copy(project().join("Cargo.toml"), s.root.join("Cargo.toml")).unwrap();
    for name in ["LICENSE", "AGENT_GUIDE.md"] {
        fs::copy(project().join(name), s.root.join(name)).unwrap();
    }
    s
}

fn prepare(s: &Sandbox) {
    for (platform, target) in PLATFORMS {
        let dir = s.root.join("target").join(target).join("release");
        fs::create_dir_all(&dir).unwrap();
        let filename = if platform.starts_with("windows") {
            "ticket.exe"
        } else {
            "ticket"
        };
        fs::write(dir.join(filename), format!("{platform} fixture")).unwrap();
        success(command(s).env("TARGET", target).env("ASSET", platform), "");
    }
}

#[test]
fn all_five_assets_are_packaged_and_checksums_verify() {
    let s = setup();
    prepare(&s);
    success(command(&s).arg("--checksums"), "");
    let manifest = fs::read_to_string(s.root.join("dist/SHA256SUMS")).unwrap();
    assert_eq!(manifest.lines().count(), 7);
    for (platform, _) in PLATFORMS {
        assert!(manifest.contains(platform));
    }
    let mut verify = Command::new("shasum");
    success(
        verify
            .args(["-a", "256", "-c", "SHA256SUMS"])
            .current_dir(s.root.join("dist")),
        "",
    );
}

#[test]
fn package_rejects_wrong_tag_target_missing_extra_and_empty_assets() {
    let s = setup();
    failure(
        command(&s)
            .env("GITHUB_REF_TYPE", "tag")
            .env("GITHUB_REF_NAME", "v999.0.0"),
    );
    failure(
        command(&s)
            .env("TARGET", "invalid")
            .env("ASSET", "linux-arm64"),
    );
    failure(
        command(&s)
            .env("TARGET", PLATFORMS[0].1)
            .env("ASSET", PLATFORMS[0].0),
    );
    failure(command(&s).arg("--checksums"));
    prepare(&s);
    let extra = s.root.join("dist/unexpected");
    fs::write(&extra, "extra").unwrap();
    failure(command(&s).arg("--checksums"));
    fs::remove_file(extra).unwrap();
    let first = s.root.join(format!(
        "dist/ticket-v{}-macos-arm64",
        env!("CARGO_PKG_VERSION")
    ));
    fs::write(first, "").unwrap();
    failure(command(&s).arg("--checksums"));
    assert!(!s.root.join("dist/SHA256SUMS").exists());
}
