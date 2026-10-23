mod common;
use common::*;
use std::fs;
use std::process::Command;

// Known SHA-256 fixture digests, independent of the installer's checksum tool.
const BINARY: &str = "abc";
const LICENSE: &str = "";
const BINARY_HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const LICENSE_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

struct Installer {
    sandbox: Sandbox,
    downloads: std::path::PathBuf,
    destination: std::path::PathBuf,
    bin: std::path::PathBuf,
}

impl Installer {
    fn new() -> Self {
        let sandbox = Sandbox::new();
        let downloads = sandbox.root.join("downloads");
        let destination = sandbox.root.join("install space");
        let bin = sandbox.root.join("bin");
        for p in [&downloads, &destination, &bin] {
            fs::create_dir(p).unwrap();
        }
        Self {
            sandbox,
            downloads,
            destination,
            bin,
        }
    }
    fn prepare(&self, platform: &str) -> String {
        let suffix = if platform.starts_with("windows") {
            ".exe"
        } else {
            ""
        };
        let asset = format!("ticket-v12.34.56-{platform}{suffix}");
        fs::write(self.downloads.join(&asset), BINARY).unwrap();
        fs::write(self.downloads.join("LICENSE"), LICENSE).unwrap();
        fs::write(
            self.downloads.join("SHA256SUMS"),
            format!("{BINARY_HASH}  {asset}\n{LICENSE_HASH}  LICENSE\n"),
        )
        .unwrap();
        asset
    }
    fn command(&self) -> Command {
        #[cfg(unix)]
        let mut cmd = {
            let mut c = Command::new("bash");
            c.arg("-c").arg(snippet("README.md", "install-unix"));
            c
        };
        #[cfg(windows)]
        let mut cmd = {
            let script = self.sandbox.root.join("install-test.ps1");
            fs::write(
                &script,
                format!(
                    "{}\n{}",
                    include_str!("support/mock_http.ps1"),
                    snippet("README.md", "install-windows")
                ),
            )
            .unwrap();
            let mut c = Command::new("pwsh");
            c.args(["-NoProfile", "-NonInteractive", "-File"])
                .arg(script);
            c
        };
        let path = std::env::join_paths(std::iter::once(self.bin.clone()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        cmd.current_dir(&self.sandbox.root)
            .env("PATH", path)
            .env("TICKET_INSTALL_DIR", &self.destination)
            .env("MOCK_DOWNLOADS", &self.downloads)
            .env("MOCK_TAG", "v12.34.56")
            .env("MOCK_FAIL", "")
            .env("MOCK_OS", "Darwin")
            .env("MOCK_CPU", "arm64");
        cmd
    }
    fn verify(&self) {
        let name = if cfg!(windows) {
            "ticket.exe"
        } else {
            "ticket"
        };
        assert_eq!(
            fs::read_to_string(self.destination.join(name)).unwrap(),
            BINARY
        );
        assert_eq!(
            fs::read_to_string(self.destination.join("ticket.LICENSE")).unwrap(),
            LICENSE
        );
        for entry in fs::read_dir(&self.destination).unwrap() {
            assert!(
                !entry
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .starts_with(".ticket")
            );
        }
    }
    #[cfg(unix)]
    fn mocks(&self) {
        executable(
            &self.bin.join("uname"),
            "#!/bin/sh\nif [ \"$1\" = -s ]; then printf '%s\\n' \"$MOCK_OS\"; else printf '%s\\n' \"$MOCK_CPU\"; fi\n",
        );
        executable(&self.bin.join("curl"), include_str!("support/mock_curl.sh"));
    }
}

#[cfg(unix)]
#[test]
fn unix_platform_selection_and_update() {
    let i = Installer::new();
    i.mocks();
    for (os, cpu, platform) in [
        ("Darwin", "arm64", "macos-arm64"),
        ("Darwin", "x86_64", "macos-x86_64"),
        ("Linux", "aarch64", "linux-arm64"),
        ("Linux", "x86_64", "linux-x86_64"),
    ] {
        i.prepare(platform);
        fs::write(i.destination.join("ticket"), "old binary").unwrap();
        success(i.command().env("MOCK_OS", os).env("MOCK_CPU", cpu), "");
        i.verify();
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(i.destination.join("ticket"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
}

#[test]
fn installer_checksums_fail_without_replacing_existing_binary() {
    let i = Installer::new();
    #[cfg(unix)]
    i.mocks();
    let platform = if cfg!(windows) {
        "windows-x86_64"
    } else {
        "macos-arm64"
    };
    let binary = if cfg!(windows) {
        "ticket.exe"
    } else {
        "ticket"
    };
    for license in [false, true] {
        let asset = i.prepare(platform);
        fs::write(i.destination.join(binary), "old binary").unwrap();
        fs::write(
            i.downloads.join(if license { "LICENSE" } else { &asset }),
            "corrupt",
        )
        .unwrap();
        failure(&mut i.command());
        assert_eq!(
            fs::read_to_string(i.destination.join(binary)).unwrap(),
            "old binary"
        );
    }
}

#[test]
fn installer_rejects_missing_and_duplicate_checksums() {
    let i = Installer::new();
    #[cfg(unix)]
    i.mocks();
    i.prepare(if cfg!(windows) {
        "windows-x86_64"
    } else {
        "macos-arm64"
    });
    let manifest = fs::read_to_string(i.downloads.join("SHA256SUMS")).unwrap();
    for invalid in [
        String::new(),
        format!("{manifest}{manifest}"),
        format!("{LICENSE_HASH}  LICENSE\n"),
    ] {
        fs::write(i.downloads.join("SHA256SUMS"), invalid).unwrap();
        failure(&mut i.command());
        assert!(!i.destination.join("ticket").exists());
        assert!(!i.destination.join("ticket.exe").exists());
    }
}

#[test]
fn installer_rejects_http_errors_unexpected_tags_and_directory_collisions() {
    let i = Installer::new();
    #[cfg(unix)]
    i.mocks();
    i.prepare(if cfg!(windows) {
        "windows-x86_64"
    } else {
        "macos-arm64"
    });
    for step in ["view", "download"] {
        failure(i.command().env("MOCK_FAIL", step));
    }
    failure(i.command().env("MOCK_TAG", "../invalid"));
    assert_eq!(fs::read_dir(&i.destination).unwrap().count(), 0);
    let name = if cfg!(windows) {
        "ticket.exe"
    } else {
        "ticket"
    };
    fs::create_dir(i.destination.join(name)).unwrap();
    failure(&mut i.command());
}

#[cfg(unix)]
#[test]
fn unix_installer_rejects_unsupported_platform() {
    let i = Installer::new();
    i.mocks();
    i.prepare("macos-arm64");
    contains(
        &failure(i.command().env("MOCK_CPU", "riscv64")),
        &["Unsupported system"],
    );
}

#[cfg(windows)]
#[test]
fn windows_install_and_update() {
    let i = Installer::new();
    i.prepare("windows-x86_64");
    fs::write(i.destination.join("ticket.exe"), "old binary").unwrap();
    success(&mut i.command(), "");
    i.verify();
}
