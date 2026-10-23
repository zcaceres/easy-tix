use std::collections::{BTreeMap, hash_map::RandomState};
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, String>;

fn io_error(path: &Path, error: std::io::Error) -> String {
    format!("{}: {error}", path.display())
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Clone)]
pub struct Ticket {
    pub fields: Vec<(String, String)>,
    pub body: String,
    pub modified: SystemTime,
}

impl Ticket {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            body: String::new(),
            modified: UNIX_EPOCH,
        }
    }

    pub fn parse(text: &str, filename_id: &str) -> Result<Self> {
        let mut ticket = Self::new();
        let mut lines = text.split_inclusive('\n');
        if lines.next().map(str::trim_end) != Some("---") {
            return Err(format!("invalid frontmatter in {filename_id}"));
        }
        let mut closed = false;
        for line in lines.by_ref() {
            let line = line.trim_end_matches(['\n', '\r']);
            if line == "---" {
                closed = true;
                break;
            }
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| format!("invalid metadata in {filename_id}"))?;
            if key.is_empty() || ticket.fields.iter().any(|(k, _)| k == key) {
                return Err(format!(
                    "invalid or duplicate field '{key}' in {filename_id}"
                ));
            }
            ticket.fields.push((key.into(), value.trim_start().into()));
        }
        if !closed || ticket.get("id") != filename_id {
            return Err(format!(
                "missing frontmatter terminator or mismatched ID in {filename_id}"
            ));
        }
        ticket.body = lines.collect();
        Ok(ticket)
    }

    pub fn id(&self) -> &str {
        self.get("id")
    }
    pub fn get(&self, field: &str) -> &str {
        self.fields
            .iter()
            .find(|(key, _)| key == field)
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    }
    pub fn set(&mut self, field: &str, value: &str) {
        if let Some((_, existing)) = self.fields.iter_mut().find(|(key, _)| key == field) {
            *existing = value.into();
        } else {
            self.fields.push((field.into(), value.into()));
        }
    }
    pub fn remove(&mut self, field: &str) {
        self.fields.retain(|(key, _)| key != field);
    }

    pub fn set_title(&mut self, title: &str) -> Result<()> {
        let headings = markdown_headings(&self.body)?;
        let titles: Vec<_> = headings.iter().filter(|h| h.level == 1).collect();
        match titles.as_slice() {
            [] => self.body.insert_str(0, &format!("# {title}\n\n")),
            [heading] => self
                .body
                .replace_range(heading.start..heading.end, &format!("# {title}\n")),
            _ => return Err("multiple top-level titles; cannot safely update Markdown".into()),
        }
        Ok(())
    }

    /// Replace only the description or one named level-two section, never notes.
    /// Byte offsets preserve the rest of the document, including CRLF and code.
    pub fn set_section(&mut self, name: &str, value: &str) -> Result<()> {
        let headings = markdown_headings(&self.body)?;
        let titles: Vec<_> = headings.iter().filter(|h| h.level == 1).collect();
        if titles.len() != 1 {
            return Err("expected one top-level title; cannot safely update Markdown".into());
        }
        let title = titles[0];
        let sections: Vec<_> = headings
            .iter()
            .filter(|h| h.level == 2 && h.start >= title.end)
            .collect();
        if name.is_empty() {
            let end = sections.first().map_or(self.body.len(), |h| h.start);
            let replacement = if value.is_empty() {
                "\n".into()
            } else {
                format!("\n{value}\n\n")
            };
            self.body.replace_range(title.end..end, &replacement);
            return Ok(());
        }
        let matches: Vec<_> = sections.iter().filter(|h| h.title == name).collect();
        let replacement = if value.is_empty() {
            String::new()
        } else {
            format!("## {name}\n\n{value}\n\n")
        };
        match matches.as_slice() {
            [] => {
                if !replacement.is_empty() {
                    // Insert new editable sections before Notes, leaving existing notes intact.
                    let at = sections
                        .iter()
                        .find(|h| h.title == "Notes")
                        .map_or(self.body.len(), |h| h.start);
                    self.body.insert_str(at, &format!("\n{replacement}"));
                }
            }
            [heading] => {
                let end = sections
                    .iter()
                    .find(|h| h.start > heading.start)
                    .map_or(self.body.len(), |h| h.start);
                self.body.replace_range(heading.start..end, &replacement);
            }
            _ => {
                return Err(format!(
                    "multiple '{name}' sections; cannot safely update Markdown"
                ));
            }
        }
        Ok(())
    }

    pub fn array(&self, field: &str) -> Vec<String> {
        self.get(field)
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }
    pub fn set_array(&mut self, field: &str, values: &[String]) {
        self.set(field, &format!("[{}]", values.join(", ")));
    }
    pub fn title(&self) -> &str {
        self.body
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or("")
    }
    pub fn serialize(&self) -> String {
        let mut text = String::from("---\n");
        for (key, value) in &self.fields {
            text.push_str(&format!("{key}: {value}\n"));
        }
        text.push_str("---\n");
        text.push_str(&self.body);
        text
    }
}

struct Heading<'a> {
    start: usize,
    end: usize,
    level: u8,
    title: &'a str,
}

fn markdown_headings(body: &str) -> Result<Vec<Heading<'_>>> {
    let mut headings = Vec::new();
    let mut offset = 0;
    let mut fence = None;
    for line in body.split_inclusive('\n') {
        let end = offset + line.len();
        let text = line.trim_end_matches(['\r', '\n']);
        let indent = text.bytes().take_while(|b| *b == b' ').count();
        if indent <= 3 {
            let text = &text[indent..];
            let first = text.as_bytes().first().copied().unwrap_or_default();
            let run = text.bytes().take_while(|b| *b == first).count();
            if let Some((marker, length)) = fence {
                if first == marker && run >= length && text[run..].trim().is_empty() {
                    fence = None;
                }
            } else if (first == b'`' || first == b'~') && run >= 3 {
                fence = Some((first, run));
            } else if let Some((level, title)) = text
                .strip_prefix("# ")
                .map(|s| (1, s))
                .or_else(|| text.strip_prefix("## ").map(|s| (2, s)))
            {
                headings.push(Heading {
                    start: offset,
                    end,
                    level,
                    title: title.trim(),
                });
            }
        }
        offset = end;
    }
    if fence.is_some() {
        return Err("unclosed Markdown code fence; cannot safely update Markdown".into());
    }
    Ok(headings)
}

pub struct Store {
    pub tickets: BTreeMap<String, Ticket>,
    directory: PathBuf,
}

impl Store {
    pub fn open(create: bool) -> Result<Self> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let explicit = std::env::var_os("TICKETS_DIR").filter(|s| !s.is_empty());
        let directory = if let Some(path) = explicit {
            let path = PathBuf::from(path);
            if !create && !path.is_dir() {
                return Err(format!(
                    "tickets directory '{}' does not exist",
                    path.display()
                ));
            }
            path
        } else if let Some(path) = cwd
            .ancestors()
            .map(|p| p.join(".tickets"))
            .find(|p| p.is_dir())
        {
            path
        } else if create {
            cwd.join(".tickets")
        } else {
            return Err("no .tickets directory found (searched parent directories)\nRun 'ticket create' to initialize, or set TICKETS_DIR".into());
        };
        let mut store = Self {
            directory,
            tickets: BTreeMap::new(),
        };
        if !store.directory.exists() {
            return Ok(store);
        }
        for entry in fs::read_dir(&store.directory).map_err(|e| io_error(&store.directory, e))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "ticket files must be regular files: {}",
                    path.display()
                ));
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| valid_id(s))
                .ok_or_else(|| format!("invalid ticket filename: {}", path.display()))?;
            let text = fs::read_to_string(&path).map_err(|e| io_error(&path, e))?;
            let mut ticket = Ticket::parse(&text, id)?;
            ticket.modified = metadata.modified().map_err(|e| io_error(&path, e))?;
            store.tickets.insert(id.into(), ticket);
        }
        Ok(store)
    }

    pub fn resolve(&self, query: &str) -> Result<String> {
        let query = query.trim();
        if !valid_id(query) {
            return Err(format!("invalid ticket ID '{query}'"));
        }
        if self.tickets.contains_key(query) {
            return Ok(query.into());
        }
        let mut matches = self.tickets.keys().filter(|id| id.contains(query));
        match (matches.next(), matches.next()) {
            (Some(id), None) => Ok(id.clone()),
            (Some(_), Some(_)) => Err(format!("ambiguous ID '{query}' matches multiple tickets")),
            _ => Err(format!("ticket '{query}' not found")),
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        // Resolve incoming relationships explicitly, never cascade to other files.
        let mut references = Vec::new();
        for ticket in self.tickets.values().filter(|t| t.id() != id) {
            for field in ["deps", "links"] {
                if ticket.array(field).iter().any(|value| value == id) {
                    references.push(format!("{} ({field})", ticket.id()));
                }
            }
            if ticket.get("parent") == id {
                references.push(format!("{} (parent)", ticket.id()));
            }
        }
        if !references.is_empty() {
            return Err(format!(
                "cannot delete {id}: referenced by {}; remove these relationships first",
                references.join(", ")
            ));
        }
        let path = self.directory.join(format!("{id}.md"));
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "ticket files must be regular files: {}",
                path.display()
            ));
        }
        fs::remove_file(&path).map_err(|e| io_error(&path, e))
    }

    pub fn create(&mut self, mut ticket: Ticket) -> Result<String> {
        fs::create_dir_all(&self.directory).map_err(|e| io_error(&self.directory, e))?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let name: String = cwd
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let prefix = if name.is_empty() {
            "tix"
        } else {
            &name[..name.len().min(3)]
        };
        // RandomState supplies OS-seeded randomness without a runtime dependency.
        // Exclusive creation, not randomness, is the collision safety guarantee.
        for attempt in 0..65536_u32 {
            let id = format!("{prefix}-{:04x}", random(attempt) & 0xffff);
            let path = self.directory.join(format!("{id}.md"));
            ticket.set("id", &id);
            // Keep id first, matching the existing on-disk layout.
            let index = ticket.fields.iter().position(|(k, _)| k == "id").unwrap();
            let pair = ticket.fields.remove(index);
            ticket.fields.insert(0, pair);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    let result = file
                        .write_all(ticket.serialize().as_bytes())
                        .and_then(|_| file.sync_all());
                    if let Err(error) = result {
                        drop(file);
                        let _ = fs::remove_file(&path);
                        return Err(io_error(&path, error));
                    }
                    return Ok(id);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error(&path, e)),
            }
        }
        Err("could not allocate an unused ticket ID".into())
    }

    pub fn save(&self, id: &str) -> Result<()> {
        let path = self.directory.join(format!("{id}.md"));
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "ticket files must be regular files: {}",
                path.display()
            ));
        }
        for attempt in 0..100 {
            let tmp = self
                .directory
                .join(format!(".{id}.{:016x}.tmp", random(attempt)));
            let mut file = match OpenOptions::new().write(true).create_new(true).open(&tmp) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error(&tmp, e)),
            };
            let result = file
                .set_permissions(metadata.permissions())
                .and_then(|_| file.write_all(self.tickets[id].serialize().as_bytes()))
                .and_then(|_| file.sync_all());
            drop(file);
            let result = result.and_then(|_| fs::rename(&tmp, &path));
            if result.is_err() {
                let _ = fs::remove_file(&tmp);
            }
            return result.map_err(|e| io_error(&path, e));
        }
        Err("could not allocate temporary file".into())
    }
}

fn random(attempt: u32) -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    attempt.hash(&mut hasher);
    hasher.finish()
}

pub fn timestamp() -> String {
    iso_timestamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
}

fn iso_timestamp(seconds: u64) -> String {
    // Gregorian civil date conversion, using 400-year eras from the Unix epoch.
    let days = (seconds / 86400) as i64 + 719468;
    let era = days / 146097;
    let day_of_era = days - era * 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_markdown_is_not_overwritten() {
        for body in [
            "# Title\n\n```\n## Notes\nunfinished code\n",
            "# Title\n\n## Design\none\n## Design\ntwo\n",
            "# Title\n# Second title\n## Notes\nkeep\n",
        ] {
            let mut ticket = Ticket::new();
            ticket.body = body.into();
            assert!(ticket.set_section("Design", "new").is_err());
            assert_eq!(ticket.body, body);
        }
    }

    #[test]
    fn utc_calendar() {
        assert_eq!(iso_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_timestamp(951782400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_timestamp(1709251199), "2024-02-29T23:59:59Z");
    }
    #[test]
    fn body_and_unknown_fields_survive_updates() {
        let mut t = Ticket::parse("---\nid: a-1234\nstatus: open\ncustom: literal / & \\\n---\n# Title\n---\nstatus: body\n", "a-1234").unwrap();
        t.set("status", "closed");
        assert!(t.serialize().contains("custom: literal / & \\\n"));
        assert!(t.serialize().ends_with("---\nstatus: body\n"));
        assert_eq!(t.get("status"), "closed");
    }
    #[test]
    fn malformed_metadata_rejected() {
        for text in [
            "no header",
            "---\nid: other\n---\n",
            "---\nid: a\nid: a\n---\n",
            "---\nid: a\n",
        ] {
            assert!(Ticket::parse(text, "a").is_err());
        }
    }
}
