mod common;
use common::*;
use std::fs;

fn timestamp_shape(value: &str) {
    assert_eq!(value.len(), 20, "{value}");
    for (i, byte) in value.bytes().enumerate() {
        match i {
            4 | 7 => assert_eq!(byte, b'-'),
            10 => assert_eq!(byte, b'T'),
            13 | 16 => assert_eq!(byte, b':'),
            19 => assert_eq!(byte, b'Z'),
            _ => assert!(byte.is_ascii_digit()),
        }
    }
}

#[test]
fn creation_defaults_and_all_options() {
    let s = Sandbox::new();
    let id = s.create(&[]);
    let text = s.text(&id);
    contains(
        &text,
        &[
            "# Untitled",
            "status: open",
            "priority: 2",
            "type: task",
            "deps: []",
            "links: []",
        ],
    );
    timestamp_shape(
        text.lines()
            .find_map(|l| l.strip_prefix("created: "))
            .unwrap(),
    );
    let child = s.create(&[
        "Full ticket",
        "-d",
        "Description",
        "--design",
        "Design text",
        "--acceptance",
        "Acceptance text",
        "-t",
        "bug",
        "-p",
        "0",
        "-a",
        "John Doe",
        "--external-ref",
        "JIRA-123",
        "--parent",
        &id,
        "--tags",
        "ui,backend",
    ]);
    contains(
        &s.text(&child),
        &[
            "# Full ticket",
            "Description",
            "## Design",
            "Design text",
            "## Acceptance Criteria",
            "Acceptance text",
            "type: bug",
            "priority: 0",
            "assignee: John Doe",
            "external-ref: JIRA-123",
            &format!("parent: {id}"),
            "tags: [ui, backend]",
        ],
    );
}

#[test]
fn invalid_creation_leaves_no_tickets() {
    let s = Sandbox::new();
    for args in [
        vec!["-p", "9"],
        vec!["-t", "unknown"],
        vec!["--tags", "ui\nstatus: closed"],
        vec!["-a", "Alice\rBob"],
        vec!["--parent", "../outside"],
        vec!["-d"],
        vec!["--tags", "[bad]"],
        vec!["--unknown", "x"],
    ] {
        let mut cmd = vec!["create", "Bad"];
        cmd.extend(args);
        s.fail(&cmd);
    }
    assert!(s.files().is_empty());
}

#[test]
fn status_commands_and_partial_ids() {
    let s = Sandbox::new();
    s.fixture("test-9999", "Test ticket", 2);
    for (command, status) in [
        ("start", "in_progress"),
        ("close", "closed"),
        ("reopen", "open"),
    ] {
        assert_eq!(
            s.ok(&[command, "9999"]),
            format!("Updated test-9999 -> {status}\n")
        );
        contains(&s.text("test-9999"), &[&format!("status: {status}")]);
    }
    for status in ["in_progress", "closed", "open"] {
        assert_eq!(
            s.ok(&["status", "9999", status]),
            format!("Updated test-9999 -> {status}\n")
        );
    }
    contains(
        &s.fail(&["status", "9999", "invalid"]),
        &["invalid status 'invalid'", "open in_progress closed"],
    );
    contains(
        &s.fail(&["status", "nonexistent", "open"]),
        &["ticket 'nonexistent' not found"],
    );
    s.fail(&["status", "9999"]);
}

#[test]
fn id_resolution_exact_prefix_suffix_substring_and_ambiguity() {
    let s = Sandbox::new();
    s.fixture("abc-1234", "Long ID ticket", 2);
    for id in ["abc-1234", "1234", "abc", "c-12", "  abc-1234  "] {
        contains(&s.ok(&["show", id]), &["id: abc-1234"]);
    }
    s.fixture("abc-5678", "Another", 2);
    contains(
        &s.fail(&["show", "abc"]),
        &["ambiguous ID 'abc' matches multiple tickets"],
    );
    s.fixture("abc", "Short ID ticket", 2);
    contains(&s.ok(&["show", "abc"]), &["id: abc\n", "Short ID ticket"]);
    excludes(&s.ok(&["show", "abc"]), &["Long ID ticket"]);
    for id in ["../outside", "*", "", "/tmp/example", "nonexistent"] {
        for args in [
            vec!["show", id],
            vec!["add-note", id, "bad"],
            vec!["delete", id],
            vec!["update", id, "--title", "Wrong"],
        ] {
            s.fail(&args);
        }
    }
}

#[test]
fn directory_discovery_overrides_and_initialization() {
    let s = Sandbox::new();
    // A fresh workspace must not borrow an unrelated ancestor store.
    for cwd in [s.root.clone(), s.root.join("orphan/deep/path")] {
        fs::create_dir_all(&cwd).unwrap();
        let error = failure(
            s.command(&["ready"])
                .env_remove("TICKETS_DIR")
                .current_dir(&cwd),
        );
        contains(&error, &["no .tickets directory found"]);
        success(
            s.command(&["help"])
                .env_remove("TICKETS_DIR")
                .current_dir(&cwd),
            "",
        );
    }
    let nested = s.root.join("new-project");
    fs::create_dir(&nested).unwrap();
    success(
        s.command(&["create", "First"])
            .env_remove("TICKETS_DIR")
            .current_dir(&nested),
        "",
    );
    assert!(nested.join(".tickets").is_dir());
    s.fixture("test-0001", "Root ticket", 2);
    s.fixture("test-0002", "Dependency", 2);
    for subdir in ["", "src/components", "src/components/ui"] {
        let cwd = s.root.join(subdir);
        fs::create_dir_all(&cwd).unwrap();
        let list = success(
            s.command(&["ls"])
                .env_remove("TICKETS_DIR")
                .current_dir(&cwd),
            "",
        );
        contains(&list, &["test-0001"]);
        success(
            s.command(&["show", "0001"])
                .env_remove("TICKETS_DIR")
                .current_dir(&cwd),
            "",
        );
        success(
            s.command(&["dep", "0001", "0002"])
                .env_remove("TICKETS_DIR")
                .current_dir(&cwd),
            "",
        );
    }
    let other = Sandbox::new();
    other.fixture("other-0001", "Other store", 2);
    let list = success(
        s.command(&["ls"])
            .env("TICKETS_DIR", &other.store)
            .current_dir(s.root.join("src")),
        "",
    );
    contains(&list, &["other-0001"]);
    excludes(&list, &["test-0001"]);
    contains(
        &s.fail(&["show", "missing"]),
        &["ticket 'missing' not found"],
    );
}

#[test]
fn dependencies_are_resolved_idempotent_and_removable() {
    let s = Sandbox::new();
    s.fixture("dep-aaaa", "Main", 2);
    s.fixture("dep-bbbb", "Dependency", 2);
    assert_eq!(
        s.ok(&["dep", "aaaa", "bbbb"]),
        "Added dependency: dep-aaaa -> dep-bbbb\n"
    );
    contains(&s.text("dep-aaaa"), &["deps: [dep-bbbb]"]);
    assert_eq!(
        s.ok(&["dep", "aaaa", "bbbb"]),
        "Dependency already exists\n"
    );
    assert_eq!(
        s.ok(&["undep", "aaaa", "bbbb"]),
        "Removed dependency: dep-aaaa -/-> dep-bbbb\n"
    );
    contains(&s.text("dep-aaaa"), &["deps: []"]);
    contains(
        &s.fail(&["undep", "aaaa", "bbbb"]),
        &["Dependency not found"],
    );
    for args in [
        ["dep", "aaaa", "nonexistent"],
        ["dep", "nonexistent", "bbbb"],
    ] {
        contains(&s.fail(&args), &["ticket 'nonexistent' not found"]);
    }
}

#[test]
fn tree_depth_then_id_order_full_output_and_cycles() {
    let s = Sandbox::new();
    for n in [1, 5, 8, 10, 15, 20, 25] {
        s.fixture(&format!("task-{n:04}"), &format!("Task {n}"), 2);
    }
    for n in [10, 5, 8, 20, 15] {
        s.ok(&["dep", "task-0001", &format!("task-{n:04}")]);
    }
    s.ok(&["dep", "task-0020", "task-0025"]);
    s.ok(&["dep", "task-0015", "task-0025"]);
    let tree = s.ok(&["dep", "tree", "task-0001"]);
    contains(&tree, &["[open] Task 1", "├──", "└──", "task-0025"]);
    let direct: Vec<_> = tree
        .lines()
        .skip(1)
        .filter(|l| l.starts_with('├') || l.starts_with('└'))
        .map(|l| l.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(
        direct,
        [
            "task-0005",
            "task-0008",
            "task-0010",
            "task-0015",
            "task-0020"
        ]
    );
    let full = s.ok(&["dep", "tree", "--full", "task-0001"]);
    assert_eq!(full.matches("task-0025 [open]").count(), 2);
    excludes(&full, &["already shown"]);
    s.ok(&["dep", "task-0025", "task-0001"]);
    contains(
        &s.ok(&["dep", "cycle"]),
        &["Dependency cycles:", "task-0001"],
    );
    contains(
        &s.ok(&["dep", "tree", "--full", "task-0001"]),
        &["already shown"],
    );
    s.ok(&["undep", "task-0025", "task-0001"]);
    assert_eq!(s.ok(&["dep", "cycle"]), "No dependency cycles found\n");
}

#[test]
fn links_are_symmetric_idempotent_and_resolved_before_writes() {
    let s = Sandbox::new();
    for id in ["link-cccc", "link-dddd", "link-eeee"] {
        s.fixture(id, id, 2);
    }
    assert_eq!(
        s.ok(&["link", "cccc", "dddd"]),
        "Added 2 link(s) between 2 tickets\n"
    );
    contains(&s.text("link-cccc"), &["links: [link-dddd]"]);
    contains(&s.text("link-dddd"), &["links: [link-cccc]"]);
    let path = s.path("link-cccc");
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(s.ok(&["link", "cccc", "dddd"]), "All links already exist\n");
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(
        s.ok(&["link", "cccc", "dddd", "eeee"]),
        "Added 4 link(s) between 3 tickets\n"
    );
    contains(&s.text("link-eeee"), &["link-cccc", "link-dddd"]);
    assert_eq!(
        s.ok(&["unlink", "cccc", "dddd"]),
        "Removed link: link-cccc <-> link-dddd\n"
    );
    excludes(&s.text("link-cccc"), &["links: [link-dddd"]);
    contains(&s.fail(&["unlink", "cccc", "dddd"]), &["Link not found"]);
    let before = s.snapshot();
    s.fail(&["link", "cccc", "dddd", "nonexistent"]);
    assert_eq!(before, s.snapshot());
    let fresh = Sandbox::new();
    for id in ["a", "b", "c"] {
        fresh.fixture(id, id, 2);
    }
    assert_eq!(
        fresh.ok(&["link", "a", "b", "c"]),
        "Added 6 link(s) between 3 tickets\n"
    );
}

#[test]
fn listing_filters_readiness_and_closed_blockers() {
    let s = Sandbox::new();
    let one = s.create(&["One", "-a", "Alice", "--tags", "ui,backend"]);
    let two = s.create(&["Two", "-a", "Bob", "--tags", "api"]);
    let three = s.create(&["Three"]);
    s.ok(&["dep", &one, &two]);
    s.ok(&["dep", &one, &three]);
    let list = s.ok(&["ls"]);
    assert_eq!(list, s.ok(&["list"]));
    contains(
        &list,
        &[
            &format!("{one} [open] - One"),
            &two,
            &three,
            &format!("<- [{two}, {three}]"),
        ],
    );
    for flags in [
        vec!["-a", "Alice"],
        vec!["--assignee=Alice"],
        vec!["-T", "ui"],
        vec!["--tag=ui"],
    ] {
        let mut args = vec!["ls"];
        args.extend(flags);
        let output = s.ok(&args);
        assert_eq!(output.lines().count(), 1);
        assert!(output.starts_with(&one));
    }
    excludes(&s.ok(&["ready"]), &[&one]);
    contains(&s.ok(&["ready"]), &[&two, &three]);
    contains(&s.ok(&["blocked"]), &[&one]);
    s.ok(&["close", &three]);
    let blocked = s.ok(&["blocked"]);
    contains(&blocked, &[&format!("<- [{two}]")]);
    excludes(&blocked, &[&three]);
    s.ok(&["close", &two]);
    contains(&s.ok(&["ready"]), &[&one]);
    excludes(&s.ok(&["ready"]), &[&two, &three]);
    assert_eq!(s.ok(&["blocked"]), "");
    contains(&s.ok(&["ls", "--status=closed"]), &[&two, &three]);
    let open = s.ok(&["ls", "--status=open"]);
    assert_eq!(open.lines().count(), 1);
    assert!(open.starts_with(&one));
    s.ok(&["reopen", &two]);
    s.ok(&["close", &one]);
    excludes(&s.ok(&["blocked"]), &[&one]);
    let closed = s.ok(&["closed"]);
    contains(&closed, &[&one, &three, "[closed]"]);
    excludes(&closed, &[&two]);
    assert_eq!(s.ok(&["closed", "--limit=1"]).lines().count(), 1);
}

#[test]
fn ready_priority_sort_and_missing_dependencies() {
    let s = Sandbox::new();
    for (id, priority) in [("ready-003", 3), ("ready-001", 1), ("ready-002", 1)] {
        s.fixture(id, id, priority);
    }
    let result = s.ok(&["ready"]);
    let ids: Vec<_> = result
        .lines()
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(ids, ["ready-001", "ready-002", "ready-003"]);
    contains(&result, &["[P1][open]"]);
    s.ok(&["dep", "ready-001", "ready-002"]);
    fs::remove_file(s.path("ready-002")).unwrap();
    contains(&s.ok(&["blocked"]), &["ready-001"]);
    excludes(&s.ok(&["ready"]), &["ready-001"]);
    contains(&s.ok(&["show", "ready-001"]), &["[missing]"]);
}

#[test]
fn empty_listings_and_closed_paths_order_limits_and_full_search() {
    let mut s = Sandbox::new();
    let dirname = if cfg!(windows) {
        "project space's store"
    } else {
        "project space's\nstore"
    };
    s.store = s.root.join(dirname).join(".tickets");
    fs::create_dir_all(&s.store).unwrap();
    for cmd in ["ls", "ready", "blocked", "closed"] {
        assert_eq!(s.ok(&[cmd]), "");
    }
    let older = s.create(&["Older", "--tags", "old"]);
    let newer = s.create(&["Newer", "--tags", "new"]);
    s.ok(&["close", &older]);
    s.ok(&["close", &newer]);
    modified(&s.path(&older), 100);
    modified(&s.path(&newer), 200);
    let result = s.ok(&["closed"]);
    let ids: Vec<_> = result
        .lines()
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(ids, [&newer, &older]);
    contains(&s.ok(&["closed", "--limit=1"]), &[&newer]);
    excludes(&s.ok(&["closed", "--limit=1"]), &[&older]);
    contains(&s.ok(&["closed", "-T", "old", "--limit=1"]), &[&older]);
    assert_eq!(s.ok(&["closed", "--limit=0"]), "");
    for value in ["bad", "-1"] {
        s.fail(&["closed", &format!("--limit={value}")]);
    }
    for i in 0..101 {
        s.fixture(&format!("open-{i:04}"), "New open ticket", 2);
    }
    contains(&s.ok(&["closed", "--limit=1"]), &[&newer]);
}

#[test]
fn show_content_and_all_relationship_sections() {
    let s = Sandbox::new();
    let parent = s.create(&["Parent"]);
    let child = s.create(&["Child", "--parent", &parent]);
    let related = s.create(&["Related"]);
    s.ok(&["dep", &child, &parent]);
    s.ok(&["link", &child, &related]);
    let shown = s.ok(&["show", &child]);
    contains(
        &shown,
        &[
            &format!("id: {child}"),
            "status: open",
            "type: task",
            "priority: 2",
            "created:",
            "# Child",
            "## Blockers",
            &format!("{parent} [open] Parent"),
            "## Linked",
            &format!("{related} [open] Related"),
            &format!("parent: {parent}  # Parent"),
        ],
    );
    let shown = s.ok(&["show", &parent]);
    contains(
        &shown,
        &[
            "## Blocking",
            "## Children",
            &format!("{child} [open] Child"),
        ],
    );
    s.ok(&["close", &parent]);
    excludes(&s.ok(&["show", &child]), &["## Blockers"]);
    contains(
        &s.fail(&["show", "nonexistent"]),
        &["ticket 'nonexistent' not found"],
    );
}

#[test]
fn notes_arguments_stdin_empty_and_existing_section() {
    let s = Sandbox::new();
    s.fixture("note-0001", "Test ticket", 2);
    for text in ["First note", "Second note", ""] {
        assert_eq!(
            s.ok(&["add-note", "0001", text]),
            "Note added to note-0001\n"
        );
    }
    success(&mut s.command(&["add-note", "0001"]), "From stdin\n");
    let body = s.text("note-0001");
    contains(&body, &["First note", "Second note", "From stdin"]);
    assert_eq!(body.matches("## Notes").count(), 1);
    for line in body.lines().filter(|l| l.starts_with("**")) {
        timestamp_shape(line.trim_matches('*'));
    }
    contains(
        &s.fail(&["add-note", "nonexistent", "My note"]),
        &["ticket 'nonexistent' not found"],
    );
}

#[test]
fn malformed_metadata_and_bad_options_fail() {
    let s = Sandbox::new();
    let id = s.create(&[]);
    for args in [
        vec!["ready", "--unknown"],
        vec!["blocked", "-a"],
        vec!["show", &id, "extra"],
        vec!["status", &id],
    ] {
        s.fail(&args);
    }
    let broken = s
        .text(&id)
        .replace("status: open", "status: open\nstatus: closed");
    fs::write(s.path(&id), &broken).unwrap();
    s.fail(&["show", &id]);
    s.fail(&["close", &id]);
    assert_eq!(s.text(&id), broken);
}

#[test]
fn legacy_body_and_unknown_metadata_round_trip() {
    let s = Sandbox::new();
    s.fixture("legacy-0001", "Unused", 1);
    let body = "# Literal | title : ✓\n\n---\nstatus: leave this alone\n$(touch injected)\n";
    let text = format!(
        "---\nid: legacy-0001\nstatus: open\ndeps: []\nlinks: []\ncreated: 2024-01-01T00:00:00Z\ntype: task\npriority: 1\ncustom: keep / & \\\nassignee: A: B\ntags: [ui, api]\n---\n{body}"
    );
    fs::write(s.path("legacy-0001"), text).unwrap();
    contains(&s.ok(&["ready", "-a", "A: B"]), &["Literal | title : ✓"]);
    s.ok(&["start", "0001"]);
    let text = s.text("legacy-0001");
    assert!(text.ends_with(body));
    contains(&text, &["custom: keep / & \\\n", "status: in_progress"]);
    assert!(!s.root.join("injected").exists());
}

#[test]
fn standalone_binary_needs_no_programs_on_path() {
    let s = Sandbox::new();
    let call = |args: &[&str]| {
        success(
            s.command(args)
                .env("PATH", "")
                .env("BASH_ENV", "must-not-be-read"),
            "",
        )
    };
    assert_eq!(
        call(&["--version"]),
        format!("ticket {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(
        call(&["agent-guide"]),
        fs::read_to_string(project().join("AGENT_GUIDE.md"))
            .unwrap()
            .replace("\r\n", "\n")
    );
    let parent = call(&["create", "Parent"]).trim().to_string();
    let child = call(&["create", "Child", "--parent", &parent])
        .trim()
        .to_string();
    call(&["dep", &child, &parent]);
    call(&["start", &parent]);
    call(&["add-note", &parent, "No subprocesses"]);
    call(&["close", &parent]);
    contains(&call(&["ready"]), &[&child]);
    contains(&call(&["closed"]), &[&parent]);
    call(&["dep", "tree", &child]);
    call(&["update", &child, "--title", "Changed"]);
    call(&["delete", &child]);
    excludes(&call(&["ls"]), &[&child]);
}

#[cfg(unix)]
#[test]
fn plugins_git_editors_and_pagers_are_not_executed_even_with_tty_output() {
    use std::os::fd::FromRawFd;
    use std::process::Stdio;
    // Test-only native PTY, replacing Python's pty module without a crate dependency.
    // musl supplies openpty in libc; only glibc needs a separate libutil.
    #[cfg_attr(all(target_os = "linux", target_env = "gnu"), link(name = "util"))]
    unsafe extern "C" {
        fn openpty(
            master: *mut i32,
            slave: *mut i32,
            name: *mut std::ffi::c_char,
            termios: *const std::ffi::c_void,
            winsize: *const std::ffi::c_void,
        ) -> i32;
    }
    let s = Sandbox::new();
    let bin = s.root.join("bin");
    fs::create_dir(&bin).unwrap();
    let marker = s.root.join("executed");
    for name in [
        "tk-create",
        "ticket-create",
        "tk-unknown",
        "ticket-unknown",
        "tk-help",
        "git",
        "launcher",
    ] {
        executable(
            &bin.join(name),
            "#!/bin/sh\nprintf executed > \"$TEST_MARKER\"\nexit 42\n",
        );
    }
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let configure = |args: &[&str]| {
        let mut c = s.command(args);
        c.env("PATH", &path).env("TEST_MARKER", &marker);
        for key in ["EDITOR", "PAGER", "TICKET_PAGER"] {
            c.env(key, bin.join("launcher"));
        }
        c
    };
    let id = success(&mut configure(&["create", "Example"]), "")
        .trim()
        .to_string();
    for args in [vec!["help"], vec!["show", &id]] {
        success(&mut configure(&args), "");
    }
    for name in ["unknown", "edit", "query", "migrate-beads", "super"] {
        failure(&mut configure(&[name, &id]));
    }
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: valid out-pointers; null name/termios/winsize request system defaults.
    assert_eq!(
        unsafe {
            openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        },
        0
    );
    // SAFETY: each successful openpty descriptor is uniquely owned by one File.
    let _master = unsafe { fs::File::from_raw_fd(master) };
    let slave = unsafe { fs::File::from_raw_fd(slave) };
    let mut child = configure(&["show", &id])
        .stdin(Stdio::null())
        .stdout(slave)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("TTY show timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!marker.exists());
}

#[test]
fn symlink_ticket_entries_are_rejected() {
    let s = Sandbox::new();
    let id = s.create(&[]);
    let outside = s.root.join("outside.md");
    fs::write(&outside, "untouched").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, s.path("escape")).unwrap();
    #[cfg(windows)]
    if let Err(e) = std::os::windows::fs::symlink_file(&outside, s.path("escape")) {
        if e.raw_os_error() == Some(1314) {
            eprintln!("Windows account lacks symlink privilege; symlink case not exercised");
            return;
        }
        panic!("{e}");
    }
    for args in [
        vec!["show", "escape"],
        vec!["add-note", "escape", "bad"],
        vec!["ls"],
        vec!["ready"],
        vec!["close", &id],
        vec!["update", "escape", "--title", "bad"],
        vec!["delete", "escape"],
    ] {
        s.fail(&args);
    }
    assert_eq!(fs::read_to_string(outside).unwrap(), "untouched");
}

#[test]
fn update_fields_clear_optionals_and_preserve_notes() {
    let s = Sandbox::new();
    let parent = s.create(&["Parent"]);
    let id = s.create(&[
        "Old",
        "-d",
        "Old description",
        "--design",
        "Old design",
        "--acceptance",
        "Old acceptance",
    ]);
    s.ok(&["add-note", &id, "Keep these notes exactly"]);
    let original = s
        .text(&id)
        .replace("status: open", "status: open\ncustom: untouched");
    fs::write(s.path(&id), &original).unwrap();
    let notes = &original[original.find("## Notes").unwrap()..];
    let created = original
        .lines()
        .find(|l| l.starts_with("created:"))
        .unwrap();
    s.ok(&[
        "update",
        &id[4..],
        "--title",
        "New",
        "-d",
        "New description",
        "--design",
        "New design",
        "--acceptance",
        "New acceptance",
        "-t",
        "bug",
        "-p",
        "0",
        "-a",
        "Alice",
        "--tags",
        "ui, api",
        "--external-ref",
        "local-42",
        "--parent",
        &parent,
        "--status",
        "in_progress",
    ]);
    let text = s.text(&id);
    contains(
        &text,
        &[
            "# New",
            "New description",
            "New design",
            "New acceptance",
            "type: bug",
            "priority: 0",
            "assignee: Alice",
            "tags: [ui, api]",
            "external-ref: local-42",
            &format!("parent: {parent}"),
            "custom: untouched",
            "status: in_progress",
            &format!("id: {id}"),
            created,
        ],
    );
    assert!(text.ends_with(notes));
    excludes(&text, &["Old"]);
    contains(&s.ok(&["ready", "-a", "Alice", "-T", "ui"]), &[&id]);
    s.ok(&[
        "update",
        &id,
        "--description",
        "",
        "--design",
        "",
        "--acceptance",
        "",
        "--assignee",
        "",
        "--external-ref",
        "",
        "--parent",
        "",
        "--tags",
        "",
    ]);
    let text = s.text(&id);
    excludes(
        &text,
        &[
            "assignee:",
            "external-ref:",
            "parent:",
            "## Design",
            "## Acceptance Criteria",
        ],
    );
    contains(&text, &["tags: []"]);
    assert!(text.ends_with(notes));
}

#[test]
fn metadata_update_leaves_entire_body_unchanged() {
    let s = Sandbox::new();
    let id = s.create(&["Body", "-d", "Literal $(touch nope)\n---\nstatus: body"]);
    s.ok(&[
        "add-note",
        &id,
        "# A heading in a note\n## Custom\nLeave this alone",
    ]);
    let before = s.text(&id);
    let body = before.splitn(3, "---\n").nth(2).unwrap();
    s.ok(&["update", &id, "-p", "4", "--status", "closed"]);
    assert_eq!(s.text(&id).splitn(3, "---\n").nth(2).unwrap(), body);
}

#[test]
fn markdown_edits_preserve_fences_unknown_sections_and_crlf() {
    let s = Sandbox::new();
    let id = s.create(&["Old"]);
    let original = s.text(&id);
    let header = original
        .splitn(3, "---\n")
        .take(2)
        .collect::<Vec<_>>()
        .join("---\n");
    let custom = "## Custom\n\n```md\n# Fake title\n## Design\n```\n~~~~\n## Notes\n~~~~\n\n";
    let notes = "## Notes\n\nKeep me\n";
    let body = format!("# Old\n\nOld description\n\n## Design\n\nold\n\n{custom}{notes}");
    fs::write(
        s.path(&id),
        format!("{header}---\n{body}").replace('\n', "\r\n"),
    )
    .unwrap();
    s.ok(&[
        "update",
        &id,
        "--title",
        "New",
        "--description",
        "new description",
        "--design",
        "new design",
        "--acceptance",
        "new acceptance",
    ]);
    let text = s.text(&id);
    contains(
        &text,
        &[
            &custom.replace('\n', "\r\n"),
            "## Acceptance Criteria\n\nnew acceptance",
        ],
    );
    assert!(text.ends_with(&notes.replace('\n', "\r\n")));
    assert_eq!(text.matches("## Design").count(), 2);
}

#[test]
fn invalid_update_never_partially_writes() {
    let s = Sandbox::new();
    let id = s.create(&[]);
    let original = s.text(&id);
    for args in [
        vec![],
        vec!["--title"],
        vec!["--unknown", "x"],
        vec!["--priority", "9"],
        vec!["--status", "blocked"],
        vec!["--type", ""],
        vec!["--tags", "[bad]"],
        vec!["--assignee", "x\nstatus: closed"],
        vec!["--parent", "missing"],
        vec!["--parent", &id],
        vec!["--id", "other"],
        vec!["--created", "yesterday"],
        vec!["bare title"],
    ] {
        let mut cmd = vec!["update", &id];
        cmd.extend(args);
        s.fail(&cmd);
        assert_eq!(s.text(&id), original);
    }
    let ambiguous = format!("{original}\n## Design\none\n## Design\ntwo\n");
    fs::write(s.path(&id), &ambiguous).unwrap();
    s.fail(&["update", &id, "--priority", "0", "--design", "ambiguous"]);
    assert_eq!(s.text(&id), ambiguous);
}

#[test]
fn reparenting_rejects_cycles_and_allows_clearing() {
    let s = Sandbox::new();
    let parent = s.create(&["Parent"]);
    let child = s.create(&["Child", "--parent", &parent]);
    s.fail(&["update", &parent, "--parent", &child]);
    excludes(&s.text(&parent), &[&format!("parent: {child}")]);
    s.ok(&["update", &child, "--parent", ""]);
    s.ok(&["update", &parent, "--parent", &child]);
    contains(&s.text(&parent), &[&format!("parent: {child}")]);
}

#[test]
fn delete_refuses_incoming_relationships_including_closed_tickets() {
    let s = Sandbox::new();
    let parent = s.create(&["Parent"]);
    let child = s.create(&["Child", "--parent", &parent]);
    let other = s.create(&["Dependent"]);
    s.ok(&["dep", &other, &parent]);
    s.ok(&["link", &other, &parent]);
    s.ok(&["close", &other]);
    let before = s.snapshot();
    contains(
        &s.fail(&["delete", &parent]),
        &[&child, &other, "(parent)", "(deps)", "(links)"],
    );
    assert_eq!(s.snapshot(), before);
    s.ok(&["undep", &other, &parent]);
    s.ok(&["unlink", &other, &parent]);
    s.ok(&["update", &child, "--parent", ""]);
    assert_eq!(
        s.ok(&["delete", &parent[4..]]),
        format!("Deleted {parent}\n")
    );
    assert!(!s.path(&parent).exists());
    s.ok(&["show", &child]);
    s.ok(&["show", &other]);
    s.fail(&["delete", &parent]);
}

#[test]
fn delete_rejects_ambiguous_ids_and_force_but_allows_outgoing_dependencies() {
    let s = Sandbox::new();
    s.fixture("abc-0001", "One", 2);
    s.fixture("abc-0002", "Two", 2);
    s.fail(&["delete", "abc"]);
    s.fail(&["update", "abc", "--title", "Wrong"]);
    s.fail(&["delete", "abc-0001", "--force"]);
    s.ok(&["dep", "abc-0001", "abc-0002"]);
    s.ok(&["delete", "abc-0001"]);
    let list = s.ok(&["ls"]);
    contains(&list, &["abc-0002"]);
    excludes(&list, &["abc-0001"]);
}

#[cfg(unix)]
#[test]
fn documented_agent_workflow_runs_verbatim() {
    let s = Sandbox::new();
    fs::copy(binary(), s.root.join("ticket")).unwrap();
    let script = snippet("AGENT_GUIDE.md", "agent-demo");
    let mut cmd = std::process::Command::new("bash");
    let output = success(
        cmd.arg("-c")
            .arg(script)
            .current_dir(&s.root)
            .env_remove("TICKETS_DIR"),
        "",
    );
    assert_eq!(s.files().len(), 2);
    let mut prerequisite = String::new();
    let mut delivery = String::new();
    for file in s.files() {
        let text = fs::read_to_string(&file).unwrap();
        contains(&text, &["status: closed", "## Notes", "Demo only:"]);
        if text.contains("# Demo: define interface") {
            prerequisite = file.file_stem().unwrap().to_str().unwrap().into();
        } else {
            delivery = text;
        }
    }
    assert!(!prerequisite.is_empty());
    contains(&delivery, &[&format!("deps: [{prerequisite}]")]);
    contains(
        &output,
        &[
            &format!("{prerequisite} [P2][open]"),
            &format!("<- [{prerequisite}]"),
        ],
    );
}
