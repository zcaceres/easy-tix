mod storage;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, IsTerminal, Read, Write};
use storage::{Result, Store, Ticket, timestamp};

const HELP: &str = include_str!("help.txt");

fn main() {
    let result = std::env::args_os()
        .skip(1)
        .map(|a| {
            a.into_string()
                .map_err(|_| "Arguments must be valid UTF-8".to_string())
        })
        .collect::<Result<Vec<_>>>()
        .and_then(|args| run(&args));
    match result {
        Ok(output) => {
            if let Err(error) = io::stdout().lock().write_all(output.as_bytes()) {
                if error.kind() != io::ErrorKind::BrokenPipe {
                    eprintln!("Error: {error}");
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

fn arity(args: &[String], min: usize, max: usize, usage: &str) -> Result<()> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(format!("Usage: ticket {usage}"))
    }
}

fn status(value: &str) -> Result<()> {
    if ["open", "in_progress", "closed"].contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "invalid status '{value}'. Must be one of: open in_progress closed"
        ))
    }
}

fn run(args: &[String]) -> Result<String> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    let args = args.get(1..).unwrap_or_default();
    match command {
        "help" | "--help" | "-h" => return Ok(HELP.into()),
        "--version" | "version" => return Ok(format!("ticket {}\n", env!("CARGO_PKG_VERSION"))),
        "agent-guide" => return Ok(include_str!("../AGENT_GUIDE.md").into()),
        "create" | "start" | "close" | "reopen" | "status" | "dep" | "undep" | "link"
        | "unlink" | "ls" | "list" | "ready" | "blocked" | "closed" | "show" | "add-note"
        | "update" | "delete" => {}
        _ => return Err(format!("Unknown command: {command}")),
    }
    let mut store = Store::open(command == "create")?;
    match command {
        "create" => create(&mut store, args),
        "update" => update(&mut store, args),
        "delete" => {
            arity(args, 1, 1, "delete <id>")?;
            let id = store.resolve(&args[0])?;
            store.delete(&id)?;
            Ok(format!("Deleted {id}\n"))
        }
        "ls" | "list" | "ready" | "blocked" | "closed" => list(&store, command, args),
        "show" => {
            arity(args, 1, 1, "show <id>")?;
            let id = store.resolve(&args[0])?;
            Ok(show(&store, &id))
        }
        "status" | "start" | "close" | "reopen" => {
            let count = if command == "status" { 2 } else { 1 };
            arity(
                args,
                count,
                count,
                &format!(
                    "{command} <id>{}",
                    if count == 2 { " <status>" } else { "" }
                ),
            )?;
            let value = match command {
                "start" => "in_progress",
                "close" => "closed",
                "reopen" => "open",
                _ => &args[1],
            };
            status(value)?;
            let id = store.resolve(&args[0])?;
            store.tickets.get_mut(&id).unwrap().set("status", value);
            store.save(&id)?;
            Ok(format!("Updated {id} -> {value}\n"))
        }
        "add-note" => {
            arity(args, 1, usize::MAX, "add-note <id> [text]")?;
            let id = store.resolve(&args[0])?;
            let note = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                if io::stdin().is_terminal() {
                    return Err("no note provided".into());
                }
                let mut text = String::new();
                io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())?;
                text.trim_end_matches('\n').into()
            };
            let ticket = store.tickets.get_mut(&id).unwrap();
            if !ticket.body.lines().any(|l| l == "## Notes") {
                ticket.body.push_str("\n## Notes\n");
            }
            ticket
                .body
                .push_str(&format!("\n**{}**\n\n{note}\n", timestamp()));
            store.save(&id)?;
            Ok(format!("Note added to {id}\n"))
        }
        "dep" if args.first().is_some_and(|a| a == "tree") => tree(&store, &args[1..]),
        "dep" if args.first().is_some_and(|a| a == "cycle") => {
            arity(args, 1, 1, "dep cycle")?;
            cycles(&store)
        }
        "dep" | "undep" => {
            arity(args, 2, 2, &format!("{command} <id> <dep-id>"))?;
            let id = store.resolve(&args[0])?;
            let dep = store.resolve(&args[1])?;
            let ticket = store.tickets.get_mut(&id).unwrap();
            let mut deps = ticket.array("deps");
            if command == "dep" {
                if deps.contains(&dep) {
                    return Ok("Dependency already exists\n".into());
                }
                deps.push(dep.clone());
            } else if !deps.contains(&dep) {
                return Err("Dependency not found".into());
            } else {
                deps.retain(|d| d != &dep);
            }
            ticket.set_array("deps", &deps);
            store.save(&id)?;
            Ok(if command == "dep" {
                format!("Added dependency: {id} -> {dep}\n")
            } else {
                format!("Removed dependency: {id} -/-> {dep}\n")
            })
        }
        "link" | "unlink" => {
            arity(
                args,
                2,
                if command == "unlink" { 2 } else { usize::MAX },
                &format!("{command} <id> <id> [id...]"),
            )?;
            // Resolve every argument before performing any writes.
            let ids = args
                .iter()
                .map(|a| store.resolve(a))
                .collect::<Result<BTreeSet<_>>>()?;
            let mut changes = 0;
            for id in &ids {
                let before = changes;
                let ticket = store.tickets.get_mut(id).unwrap();
                let mut links = ticket.array("links");
                for other in ids.iter().filter(|other| *other != id) {
                    if command == "link" && !links.contains(other) {
                        links.push(other.clone());
                        changes += 1;
                    } else if command == "unlink" && links.contains(other) {
                        links.retain(|l| l != other);
                        changes += 1;
                    }
                }
                if changes != before {
                    ticket.set_array("links", &links);
                    store.save(id)?;
                }
            }
            if changes == 0 {
                return if command == "unlink" {
                    Err("Link not found".into())
                } else {
                    Ok("All links already exist\n".into())
                };
            }
            if command == "unlink" {
                Ok(format!(
                    "Removed link: {} <-> {}\n",
                    store.resolve(&args[0])?,
                    store.resolve(&args[1])?
                ))
            } else {
                Ok(format!(
                    "Added {changes} link(s) between {} tickets\n",
                    ids.len()
                ))
            }
        }
        _ => unreachable!(),
    }
}

type Fields = BTreeMap<&'static str, String>;

fn parse_fields(args: &[String], updating: bool) -> Result<Fields> {
    let mut fields = BTreeMap::new();
    let mut title_seen = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg.starts_with('-') {
            let key = match arg.as_str() {
                "-d" | "--description" => "description",
                "--design" => "design",
                "--acceptance" => "acceptance",
                "-t" | "--type" => "type",
                "-p" | "--priority" => "priority",
                "-a" | "--assignee" => "assignee",
                "--external-ref" => "external-ref",
                "--parent" => "parent",
                "--tags" => "tags",
                "--title" if updating => "title",
                "--status" if updating => "status",
                _ => return Err(format!("Unknown option: {arg}")),
            };
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| format!("missing value for {arg}"))?;
            fields.insert(key, value.clone());
        } else if updating {
            return Err(format!(
                "Unexpected argument: {arg}; use --title to change the title"
            ));
        } else if title_seen {
            return Err("Pass the title as one quoted argument".into());
        } else {
            fields.insert("title", arg.clone());
            title_seen = true;
        }
        i += 1;
    }
    if let Some(kind) = fields.get("type") {
        if !["bug", "feature", "task", "epic", "chore"].contains(&kind.as_str()) {
            return Err(format!("invalid type '{kind}'"));
        }
    }
    if let Some(priority) = fields.get("priority") {
        if !["0", "1", "2", "3", "4"].contains(&priority.as_str()) {
            return Err("priority must be 0-4".into());
        }
    }
    if let Some(value) = fields.get("status") {
        status(value)?;
    }
    for value in fields.iter().filter_map(|(key, value)| {
        if ["description", "design", "acceptance"].contains(key) {
            None
        } else {
            Some(value)
        }
    }) {
        if value.contains(['\n', '\r']) {
            return Err("title and metadata must be single-line values".into());
        }
    }
    if let Some(tags) = fields.get("tags") {
        parse_tags(tags)?;
    }
    Ok(fields)
}

fn parse_tags(value: &str) -> Result<Vec<String>> {
    if value.contains(['[', ']']) {
        return Err("tags cannot contain brackets".into());
    }
    Ok(value
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect())
}

fn apply_metadata(store: &Store, ticket: &mut Ticket, fields: &Fields) -> Result<()> {
    for key in ["type", "priority", "status"] {
        if let Some(value) = fields.get(key) {
            ticket.set(key, value);
        }
    }
    for key in ["assignee", "external-ref"] {
        if let Some(value) = fields.get(key) {
            if value.is_empty() {
                ticket.remove(key);
            } else {
                ticket.set(key, value);
            }
        }
    }
    if let Some(parent) = fields.get("parent") {
        if parent.is_empty() {
            ticket.remove("parent");
        } else {
            let parent = store.resolve(parent)?;
            // Keep reparenting from creating a cycle, including indirect self-parenting.
            let mut cursor = parent.as_str();
            let mut seen = BTreeSet::new();
            while !cursor.is_empty() {
                if cursor == ticket.id() || !seen.insert(cursor) {
                    return Err("parent relationship would create or join a cycle".into());
                }
                cursor = store
                    .tickets
                    .get(cursor)
                    .map(|t| t.get("parent"))
                    .unwrap_or("");
            }
            ticket.set("parent", &parent);
        }
    }
    if let Some(tags) = fields.get("tags") {
        ticket.set_array("tags", &parse_tags(tags)?);
    }
    Ok(())
}

fn create(store: &mut Store, args: &[String]) -> Result<String> {
    let fields = parse_fields(args, false)?;
    let title = fields
        .get("title")
        .map(String::as_str)
        .unwrap_or("Untitled");
    let mut ticket = Ticket::new();
    ticket.set("status", "open");
    ticket.set("deps", "[]");
    ticket.set("links", "[]");
    ticket.set("created", &timestamp());
    ticket.set("type", "task");
    ticket.set("priority", "2");
    apply_metadata(store, &mut ticket, &fields)?;
    ticket.body = format!(
        "# {}\n\n",
        if title.is_empty() { "Untitled" } else { title }
    );
    for (key, heading) in [
        ("description", ""),
        ("design", "Design"),
        ("acceptance", "Acceptance Criteria"),
    ] {
        if let Some(value) = fields.get(key).filter(|v| !v.is_empty()) {
            if !heading.is_empty() {
                ticket.body.push_str(&format!("## {heading}\n\n"));
            }
            ticket.body.push_str(value);
            ticket.body.push_str("\n\n");
        }
    }
    Ok(format!("{}\n", store.create(ticket)?))
}

fn update(store: &mut Store, args: &[String]) -> Result<String> {
    arity(args, 2, usize::MAX, "update <id> [options]")?;
    let id = store.resolve(&args[0])?;
    let fields = parse_fields(&args[1..], true)?;
    // Work on a copy: all validation and Markdown editing must succeed before save.
    let mut ticket = store.tickets[&id].clone();
    apply_metadata(store, &mut ticket, &fields)?;
    if let Some(title) = fields.get("title") {
        ticket.set_title(if title.is_empty() { "Untitled" } else { title })?;
    }
    for (key, heading) in [
        ("description", ""),
        ("design", "Design"),
        ("acceptance", "Acceptance Criteria"),
    ] {
        if let Some(value) = fields.get(key) {
            ticket.set_section(heading, value)?;
        }
    }
    store.tickets.insert(id.clone(), ticket);
    store.save(&id)?;
    Ok(format!("Updated {id}\n"))
}

#[derive(Default)]
struct Filters {
    status: String,
    assignee: String,
    tag: String,
    limit: Option<usize>,
}

impl Filters {
    fn parse(command: &str, args: &[String]) -> Result<Self> {
        let mut result = Self::default();
        let mut i = 0;
        while i < args.len() {
            let (key, value) = if let Some((key, value)) = args[i].split_once('=') {
                (key, value)
            } else if ["-a", "-T"].contains(&args[i].as_str()) {
                let key = args[i].as_str();
                i += 1;
                (
                    key,
                    args.get(i)
                        .ok_or_else(|| format!("missing value for {key}"))?
                        .as_str(),
                )
            } else {
                return Err(format!("Unknown option: {}", args[i]));
            };
            match key {
                "-a" | "--assignee" => result.assignee = value.into(),
                "-T" | "--tag" => result.tag = value.into(),
                "--status" if ["ls", "list"].contains(&command) => {
                    status(value)?;
                    result.status = value.into();
                }
                "--limit" if command == "closed" => {
                    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                        return Err("limit must be a nonnegative integer".into());
                    }
                    result.limit = Some(value.parse().map_err(|_| "limit is too large")?);
                }
                _ => return Err(format!("Unknown option: {key}")),
            }
            i += 1;
        }
        Ok(result)
    }

    fn matches(&self, ticket: &Ticket) -> bool {
        (self.status.is_empty() || ticket.get("status") == self.status)
            && (self.assignee.is_empty() || ticket.get("assignee") == self.assignee)
            && (self.tag.is_empty() || ticket.array("tags").contains(&self.tag))
    }
}

fn blockers(store: &Store, ticket: &Ticket) -> Vec<String> {
    ticket
        .array("deps")
        .into_iter()
        .filter(|id| {
            store
                .tickets
                .get(id)
                .is_none_or(|t| t.get("status") != "closed")
        })
        .collect()
}

fn list(store: &Store, command: &str, args: &[String]) -> Result<String> {
    let filters = Filters::parse(command, args)?;
    let mut tickets: Vec<_> = store
        .tickets
        .values()
        .filter(|t| filters.matches(t))
        .filter(|t| match command {
            "closed" => t.get("status") == "closed",
            "ready" | "blocked" => {
                ["open", "in_progress"].contains(&t.get("status"))
                    && (blockers(store, t).is_empty() == (command == "ready"))
            }
            _ => true,
        })
        .collect();
    match command {
        "closed" => tickets.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.id().cmp(b.id()))),
        "ready" | "blocked" => {
            tickets.sort_by_key(|t| (t.get("priority").parse::<u8>().unwrap_or(2), t.id()))
        }
        _ => {}
    }
    let mut out = String::new();
    let limit = if command == "closed" {
        filters.limit.unwrap_or(20)
    } else {
        usize::MAX
    };
    for ticket in tickets.into_iter().take(limit) {
        let priority = if ["ready", "blocked"].contains(&command) {
            format!("[P{}]", ticket.get("priority"))
        } else {
            String::new()
        };
        let deps = match command {
            "ls" | "list" => ticket.array("deps"),
            "blocked" => blockers(store, ticket),
            _ => Vec::new(),
        };
        let suffix = if deps.is_empty() {
            String::new()
        } else {
            format!(" <- [{}]", deps.join(", "))
        };
        out.push_str(&format!(
            "{:<8} {priority}[{}] - {}{suffix}\n",
            ticket.id(),
            ticket.get("status"),
            ticket.title()
        ));
    }
    Ok(out)
}

fn show(store: &Store, id: &str) -> String {
    let ticket = &store.tickets[id];
    let mut out = String::from("---\n");
    for (key, value) in &ticket.fields {
        out.push_str(&format!("{key}: {value}"));
        if key == "parent" {
            if let Some(parent) = store.tickets.get(value) {
                out.push_str(&format!("  # {}", parent.title()));
            }
        }
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(&ticket.body);
    let blocking = store
        .tickets
        .values()
        .filter(|t| t.get("status") != "closed" && t.array("deps").iter().any(|d| d == id))
        .map(|t| t.id().to_string())
        .collect();
    let children = store
        .tickets
        .values()
        .filter(|t| t.get("parent") == id)
        .map(|t| t.id().to_string())
        .collect();
    for (heading, ids) in [
        ("Blockers", blockers(store, ticket)),
        ("Blocking", blocking),
        ("Children", children),
        ("Linked", ticket.array("links")),
    ] {
        if !ids.is_empty() {
            out.push_str(&format!("\n## {heading}\n\n"));
            for related in ids {
                if let Some(t) = store.tickets.get(&related) {
                    out.push_str(&format!(
                        "- {related} [{}] {}\n",
                        t.get("status"),
                        t.title()
                    ));
                } else {
                    out.push_str(&format!("- {related} [missing]\n"));
                }
            }
        }
    }
    out
}

fn depth(store: &Store, id: &str, path: &mut BTreeSet<String>) -> Result<usize> {
    if path.len() >= 256 {
        return Err("dependency graph exceeds 256 levels".into());
    }
    if !path.insert(id.into()) {
        return Ok(0);
    }
    let mut result = 0;
    if let Some(ticket) = store.tickets.get(id) {
        for dep in ticket.array("deps") {
            result = result.max(1 + depth(store, &dep, path)?);
        }
    }
    path.remove(id);
    Ok(result)
}

fn tree(store: &Store, args: &[String]) -> Result<String> {
    let full = args.iter().any(|a| a == "--full");
    let ids: Vec<_> = args.iter().filter(|a| *a != "--full").cloned().collect();
    arity(&ids, 1, 1, "dep tree [--full] <id>")?;
    let id = store.resolve(&ids[0])?;
    let mut out = String::new();
    let mut seen = BTreeSet::new();
    let mut path = BTreeSet::new();
    // The explicit rendering stack avoids recursive formatting state.
    let mut stack = vec![(id, String::new(), String::new(), false)];
    while let Some((id, prefix, branch, leaving)) = stack.pop() {
        if leaving {
            path.remove(&id);
            continue;
        }
        if path.len() >= 256 {
            return Err("dependency graph exceeds 256 levels".into());
        }
        let label = store
            .tickets
            .get(&id)
            .map(|t| format!("[{}] {}", t.get("status"), t.title()))
            .unwrap_or("[missing]".into());
        out.push_str(&format!("{prefix}{branch}{id} {label}"));
        if path.contains(&id) || (!full && seen.contains(&id)) {
            out.push_str(" (already shown)\n");
            continue;
        }
        out.push('\n');
        path.insert(id.clone());
        seen.insert(id.clone());
        stack.push((id.clone(), String::new(), String::new(), true));
        if let Some(ticket) = store.tickets.get(&id) {
            let mut deps = Vec::new();
            for dep in ticket.array("deps") {
                deps.push((depth(store, &dep, &mut BTreeSet::new())?, dep));
            }
            deps.sort();
            let next_prefix = format!(
                "{prefix}{}",
                match branch.as_str() {
                    "├── " => "│   ",
                    "└── " => "    ",
                    _ => "",
                }
            );
            let len = deps.len();
            for (index, (_, dep)) in deps.into_iter().enumerate().rev() {
                stack.push((
                    dep,
                    next_prefix.clone(),
                    if index + 1 == len {
                        "└── "
                    } else {
                        "├── "
                    }
                    .into(),
                    false,
                ));
            }
        }
    }
    Ok(out)
}

fn cycles(store: &Store) -> Result<String> {
    fn visit(
        store: &Store,
        id: &str,
        path: &mut Vec<String>,
        done: &mut BTreeSet<String>,
        found: &mut Vec<String>,
    ) -> Result<()> {
        if let Some(index) = path.iter().position(|p| p == id) {
            let mut cycle = path[index..].to_vec();
            cycle.push(id.into());
            found.push(cycle.join(" -> "));
            return Ok(());
        }
        if done.contains(id) {
            return Ok(());
        }
        if path.len() >= 256 {
            return Err("dependency graph exceeds 256 levels".into());
        }
        if let Some(ticket) = store
            .tickets
            .get(id)
            .filter(|t| t.get("status") != "closed")
        {
            path.push(id.into());
            for dep in ticket.array("deps") {
                visit(store, &dep, path, done, found)?;
            }
            path.pop();
        }
        done.insert(id.into());
        Ok(())
    }
    let mut done = BTreeSet::new();
    let mut found = Vec::new();
    for id in store.tickets.keys() {
        visit(store, id, &mut Vec::new(), &mut done, &mut found)?;
    }
    Ok(if found.is_empty() {
        "No dependency cycles found\n".into()
    } else {
        format!("Dependency cycles:\n{}\n", found.join("\n"))
    })
}
