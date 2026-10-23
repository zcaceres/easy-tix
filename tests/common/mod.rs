#![allow(dead_code)]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn binary() -> PathBuf {
    // CI can test the exact artifact it will publish, rather than a different build.
    std::env::var_os("TICKET_SCRIPT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_ticket")))
}

pub struct Sandbox {
    pub root: PathBuf,
    pub store: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
        let unique = format!(
            "ticket_test_{}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir(&root).unwrap();
        Self {
            store: root.join(".tickets"),
            root,
        }
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(binary());
        cmd.args(args)
            .current_dir(&self.root)
            .env("TICKETS_DIR", &self.store);
        for key in ["BASH_ENV", "ENV", "SHELLOPTS", "BASHOPTS"] {
            cmd.env_remove(key);
        }
        cmd
    }

    pub fn ok(&self, args: &[&str]) -> String {
        success(&mut self.command(args), "")
    }

    pub fn fail(&self, args: &[&str]) -> String {
        failure(&mut self.command(args))
    }

    pub fn create(&self, args: &[&str]) -> String {
        let mut command = vec!["create"];
        command.extend_from_slice(args);
        let id = self.ok(&command).trim().to_owned();
        let (prefix, suffix) = id.split_once('-').expect("generated ID");
        assert!(!prefix.is_empty() && prefix.bytes().all(|c| c.is_ascii_alphanumeric()));
        assert_eq!(suffix.len(), 4);
        assert!(suffix.bytes().all(|c| c.is_ascii_hexdigit()));
        id
    }

    pub fn path(&self, id: &str) -> PathBuf {
        self.store.join(format!("{id}.md"))
    }

    pub fn text(&self, id: &str) -> String {
        fs::read_to_string(self.path(id)).unwrap()
    }

    pub fn fixture(&self, id: &str, title: &str, priority: u8) {
        fs::create_dir_all(&self.store).unwrap();
        fs::write(self.path(id), format!(
            "---\nid: {id}\nstatus: open\ndeps: []\nlinks: []\ncreated: 2024-01-01T00:00:00Z\ntype: task\npriority: {priority}\n---\n# {title}\n\nDescription\n"
        )).unwrap();
    }

    pub fn files(&self) -> Vec<PathBuf> {
        if !self.store.exists() {
            return Vec::new();
        }
        let mut files: Vec<_> = fs::read_dir(&self.store)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|s| s == "md"))
            .collect();
        files.sort();
        files
    }

    pub fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        self.files()
            .into_iter()
            .map(|p| {
                let data = fs::read(&p).unwrap();
                (p, data)
            })
            .collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn run(cmd: &mut Command, input: &str) -> Output {
    let description = format!("{cmd:?}");
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("{description}: {e}"));
    let mut stdin = child.stdin.take().unwrap();
    let input = input.as_bytes().to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    // Drain both pipes while waiting (Windows pipe buffers can be very small).
    let out = std::thread::spawn(move || {
        let mut b = Vec::new();
        stdout.read_to_end(&mut b).unwrap();
        b
    });
    let err = std::thread::spawn(move || {
        let mut b = Vec::new();
        stderr.read_to_end(&mut b).unwrap();
        b
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("command timed out: {description}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    writer.join().unwrap();
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

pub fn success(cmd: &mut Command, input: &str) -> String {
    let out = run(cmd, input);
    assert!(
        out.status.success(),
        "{cmd:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().replace("\r\n", "\n")
}

pub fn failure(cmd: &mut Command) -> String {
    let out = run(cmd, "");
    assert!(!out.status.success(), "unexpected success: {cmd:?}");
    String::from_utf8(out.stderr).unwrap()
}

pub fn contains(text: &str, parts: &[&str]) {
    for part in parts {
        assert!(text.contains(part), "missing {part:?} in:\n{text}");
    }
}

pub fn excludes(text: &str, parts: &[&str]) {
    for part in parts {
        assert!(!text.contains(part), "unexpected {part:?} in:\n{text}");
    }
}

pub fn snippet(path: &str, marker: &str) -> String {
    let text = fs::read_to_string(project().join(path)).unwrap();
    let section = text
        .split_once(&format!("<!-- {marker}:start -->"))
        .unwrap()
        .1;
    let fenced = section.split_once("```").unwrap().1;
    fenced
        .split_once('\n')
        .unwrap()
        .1
        .split_once("```")
        .unwrap()
        .0
        .to_owned()
}

#[cfg(unix)]
pub fn executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn modified(path: &Path, seconds: u64) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
        .unwrap();
}
