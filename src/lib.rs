use lazy_static::lazy_static;
use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use std::collections::{hash_map::Iter as HashMapIter, HashMap};
use std::fmt::{self, Write};
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};

pub mod db;
pub mod search;

lazy_static! {
    static ref TAGS: Regex = { Regex::new(r"^(@\w+)(\([^)]*\))?").unwrap() };
}

#[derive(Debug)]
pub enum Error {
    Misc(String),
    Other(Box<dyn::std::error::Error>),
    Io(io::Error),
}

impl From<io::Error> for Error {
    fn from(other: io::Error) -> Error {
        Error::Io(other)
    }
}

impl From<Box<dyn::std::error::Error>> for Error {
    fn from(other: Box<dyn::std::error::Error>) -> Error {
        Error::Other(other)
    }
}

impl Error {
    pub fn misc(text: impl Into<String>) -> Self {
        Error::Misc(text.into())
    }
}

// NOCOM(#sirver): Use failure here
pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Tag {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Tags {
    tags: HashMap<String, Option<String>>,
}

impl Tags {
    pub fn new() -> Self {
        Tags {
            tags: HashMap::new(),
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.tags.remove(name);
    }

    pub fn insert(&mut self, tag: Tag) {
        self.tags.insert(tag.name, tag.value);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tags.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<Tag> {
        self.tags.get(name).map(|v| Tag {
            name: name.to_string(),
            value: v.clone(),
        })
    }

    pub fn iter(&self) -> TagsIterator<'_> {
        TagsIterator {
            iter: self.tags.iter(),
        }
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }
}

pub struct TagsIterator<'a> {
    iter: HashMapIter<'a, String, Option<String>>,
}

impl<'a> Iterator for TagsIterator<'a> {
    type Item = Tag;

    fn next(&mut self) -> Option<Tag> {
        self.iter.next().map(|(k, v)| Tag {
            name: k.to_string(),
            value: v.clone(),
        })
    }
}
impl Tag {
    pub fn new(name: String, value: Option<String>) -> Self {
        Tag { name, value }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "@{}", self.name)?;
        if let Some(v) = &self.value {
            write!(f, "({})", v)?;
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Project {
    pub text: String,
    pub note: Option<Note>,
    pub tags: Tags,
    pub children: Vec<Entry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Sort {
    // Do not change ordering of the items, print them as they arrive.
    Nothing,

    // Order projects on top, i.e. before tasks.
    ProjectsFirst,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmptyLineAfterProject {
    // NOCOM(#sirver): document
    pub top_level: usize,
    pub first_level: usize,
    pub others: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PrintChildren {
    // NOCOM(#sirver): document
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PrintNotes {
    // NOCOM(#sirver): document
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum VimReadOnly {
    // NOCOM(#sirver): document
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FormatOptions {
    pub indent: usize,
    pub sort: Sort,
    pub print_children: PrintChildren,
    pub print_notes: PrintNotes,
    pub empty_line_after_project: EmptyLineAfterProject,
    pub vim_read_only: VimReadOnly,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            indent: 0,
            sort: Sort::ProjectsFirst,
            print_children: PrintChildren::Yes,
            print_notes: PrintNotes::Yes,
            empty_line_after_project: EmptyLineAfterProject {
                top_level: 1,
                first_level: 1,
                others: 0,
            },
            vim_read_only: VimReadOnly::No,
        }
    }
}

impl ToStringWithIndent for Project {
    fn append_to_string(&self, buf: &mut String, options: FormatOptions) -> fmt::Result {
        let indent_str = "\t".repeat(options.indent);
        let mut tags = self.tags.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        tags.sort();
        let tags_string = if tags.is_empty() {
            "".to_string()
        } else {
            format!(" {}", tags.join(" "))
        };
        writeln!(buf, "{}{}:{}", indent_str, self.text, tags_string)?;

        match options.print_notes {
            PrintNotes::No => (),
            PrintNotes::Yes => {
                if let Some(note) = &self.note {
                    let note_indent = "\t".repeat(options.indent + 1);
                    for line in note.text.split_terminator('\n') {
                        writeln!(buf, "{}{}", note_indent, line)?;
                    }
                }
            }
        }

        match options.print_children {
            PrintChildren::No => (),
            PrintChildren::Yes => print_entries(
                buf,
                self.children.iter().collect(),
                FormatOptions {
                    indent: options.indent + 1,
                    ..options
                },
            )?,
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Task {
    pub tags: Tags,
    pub text: String,
    // NOCOM(#sirver): that note should be a proper note structure.
    pub note: Option<String>,
}

pub trait ToStringWithIndent {
    fn append_to_string(&self, buf: &mut String, options: FormatOptions) -> fmt::Result;

    fn to_string(&self, options: FormatOptions) -> String {
        let mut s = String::new();
        self.append_to_string(&mut s, options).unwrap();
        s
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Note {
    pub text: String,
}

impl ToStringWithIndent for Note {
    fn append_to_string(&self, buf: &mut String, options: FormatOptions) -> fmt::Result {
        let indent = "\t".repeat(options.indent);
        for line in self.text.split_terminator('\n') {
            writeln!(buf, "{}{}", indent, line)?;
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Entry {
    Task(Task),
    Project(Project),
    Note(Note),
}

impl Entry {
    pub fn is_project(&self) -> bool {
        match *self {
            Entry::Project(_) => true,
            _ => false,
        }
    }
}

impl ToStringWithIndent for Entry {
    fn append_to_string(&self, buf: &mut String, options: FormatOptions) -> fmt::Result {
        match self {
            Entry::Task(t) => t.append_to_string(buf, options),
            Entry::Project(p) => p.append_to_string(buf, options),
            Entry::Note(n) => n.append_to_string(buf, options),
        }
    }
}

impl ToStringWithIndent for Task {
    fn append_to_string(&self, buf: &mut String, options: FormatOptions) -> fmt::Result {
        let indent_str = "\t".repeat(options.indent);
        let mut tags = self.tags.iter().collect::<Vec<Tag>>();
        tags.sort_by_key(|t| (t.value.is_some(), t.name.clone()));
        let tags_string = if tags.is_empty() {
            "".to_string()
        } else {
            let tag_strings = tags.iter().map(|t| t.to_string()).collect::<Vec<String>>();
            format!(" {}", tag_strings.join(" "))
        };
        writeln!(buf, "{}- {}{}", indent_str, self.text, tags_string)?;

        match options.print_notes {
            PrintNotes::No => (),
            PrintNotes::Yes => {
                if let Some(note) = &self.note {
                    let note_indent = "\t".repeat(options.indent + 1);
                    for line in note.split_terminator('\n') {
                        writeln!(buf, "{}{}", note_indent, line)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl ToStringWithIndent for [&Entry] {
    fn append_to_string(&self, buf: &mut String, options: FormatOptions) -> fmt::Result {
        print_entries(buf, self.to_vec(), options)?;
        Ok(())
    }
}

fn print_entries(
    buf: &mut String,
    mut entries: Vec<&Entry>,
    options: FormatOptions,
) -> fmt::Result {
    // Projects are bubbled to the top.
    match options.sort {
        Sort::Nothing => (),
        Sort::ProjectsFirst => entries.sort_by_key(|a| !a.is_project()),
    }

    let maybe_empty_line = |buf: &mut String, idx: usize| {
        // Only if there is a next item and that is a project do we actually print a new line.
        if let Some(s) = entries.get(idx + 1) {
            if s.is_project() {
                writeln!(buf, "")?;
            }
        }
        Ok(())
    };

    for (idx, e) in entries.iter().enumerate() {
        match e {
            Entry::Project(p) => {
                p.append_to_string(buf, options)?;
                let add_empty_line = match options.indent {
                    0 => options.empty_line_after_project.top_level,
                    1 => options.empty_line_after_project.first_level,
                    _ => options.empty_line_after_project.others,
                };
                for _ in 0..add_empty_line {
                    maybe_empty_line(buf, idx)?;
                }
            }
            Entry::Task(t) => {
                t.append_to_string(buf, options)?;
            }
            Entry::Note(n) => n.append_to_string(buf, options)?,
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
enum LineKind {
    Task,
    Project,
    Note,
}

fn is_task(line: &str) -> bool {
    line.trim_left().starts_with("- ")
}

fn indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == '\t').count()
}

fn is_project(line: &str) -> bool {
    line.trim_right().ends_with(':')
}

fn classify(line: &str) -> LineKind {
    let (without_tags, _) = extract_tags(line);
    if is_task(&without_tags) {
        LineKind::Task
    } else if is_project(&without_tags) {
        LineKind::Project
    } else {
        LineKind::Note
    }
}

// NOCOM(#sirver): should take a String
fn extract_tags(line: &str) -> (String, Tags) {
    let mut tags = Tags::new();
    let mut line = line.to_string();
    while let Some((tag, (start, end))) = find_tag(&line) {
        tags.insert(tag);
        line = line[0..start].to_string() + &line[end..line.len()];
    }
    (line, tags)
}

#[derive(Debug)]
struct LineToken {
    indent: usize,
    text: String,
    kind: LineKind,
}

fn parse_task(it: &mut Peekable<impl Iterator<Item = LineToken>>) -> Task {
    let token = it.next().unwrap();
    let (without_tags, tags) = extract_tags(&token.text);

    let note = match it.peek() {
        Some(nt) if nt.kind == LineKind::Note => Some(parse_note(it).text),
        _ => None,
    };

    Task {
        // Also trim the leading '- '
        text: without_tags.trim()[1..].trim_left().to_string(),
        tags,
        note,
    }
}

fn parse_project(it: &mut Peekable<impl Iterator<Item = LineToken>>) -> Project {
    let token = it.next().unwrap();
    let (without_tags, tags) = extract_tags(&token.text);
    let without_tags = without_tags.trim();

    let note = match it.peek() {
        Some(nt) if nt.kind == LineKind::Note => Some(parse_note(it)),
        _ => None,
    };

    let mut children = vec![];
    while let Some(nt) = it.peek() {
        if nt.indent <= token.indent {
            break;
        }
        children.push(parse_entry(it));
    }

    Project {
        // Also trim the trailing ':'
        text: without_tags[..without_tags.len() - 1].to_string(),
        note,
        tags,
        children,
    }
}

fn parse_note(it: &mut Peekable<impl Iterator<Item = LineToken>>) -> Note {
    let mut text = vec![];
    let first_indent = it.peek().unwrap().indent;
    while let Some(nt) = it.peek() {
        if nt.kind != LineKind::Note || nt.indent < first_indent {
            break;
        }
        let nt = it.next().unwrap();
        let indent = "\t".repeat(nt.indent - first_indent);
        text.push(format!("{}{}", indent, nt.text));
    }
    Note {
        text: text.join("\n"),
    }
}

fn parse_entry(it: &mut Peekable<impl Iterator<Item = LineToken>>) -> Entry {
    let token = it.peek().unwrap();
    match token.kind {
        LineKind::Task => Entry::Task(parse_task(it)),
        LineKind::Project => Entry::Project(parse_project(it)),
        LineKind::Note => Entry::Note(parse_note(it)),
    }
}

#[derive(Debug)]
pub struct TaskpaperFile {
    pub entries: Vec<Entry>,
}

#[derive(Debug)]
pub enum CommonFileKind {
    Inbox,
    Todo,
    Tickle,
    Checkout,
    Logbook,
}

impl CommonFileKind {
    fn find(&self) -> Option<PathBuf> {
        let home = dirs::home_dir().expect("HOME not set.");
        let path = match *self {
            CommonFileKind::Inbox => home.join("Dropbox/Tasks/01_inbox.taskpaper"),
            CommonFileKind::Todo => home.join("Dropbox/Tasks/02_todo.taskpaper"),
            CommonFileKind::Tickle => home.join("Dropbox/Tasks/03_tickle.taskpaper"),
            CommonFileKind::Checkout => home.join("Dropbox/Tasks/09_to_checkout.taskpaper"),
            CommonFileKind::Logbook => home.join("Dropbox/Tasks/40_logbook.taskpaper"),
        };
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    #[cfg(test)]
    fn to_path_buf(&self) -> PathBuf {
        match *self {
            CommonFileKind::Inbox => PathBuf::from("01_inbox.taskpaper"),
            CommonFileKind::Todo => PathBuf::from("02_todo.taskpaper"),
            CommonFileKind::Tickle => PathBuf::from("03_tickle.taskpaper"),
            CommonFileKind::Checkout => PathBuf::from("09_to_checkout.taskpaper"),
            CommonFileKind::Logbook => PathBuf::from("40_logbook.taskpaper"),
        }
    }
}

impl TaskpaperFile {
    pub fn new() -> Self {
        TaskpaperFile {
            entries: Vec::new(),
        }
    }

    pub fn parse_common_file(kind: CommonFileKind) -> Result<Self> {
        Self::parse_file(kind.find().expect("Common file not found!"))
    }

    pub fn overwrite_common_file(
        &self,
        kind: CommonFileKind,
        options: FormatOptions,
    ) -> Result<()> {
        self.write(kind.find().expect("Common file not found!"), options)
    }

    pub fn parse_file(path: impl AsRef<Path>) -> Result<Self> {
        let text = ::std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn parse(input: &str) -> Result<Self> {
        let lines = input
            .trim()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| LineToken {
                indent: indent(line),
                kind: classify(line),
                text: line.trim().to_string(),
            });

        let mut entries = Vec::new();
        let mut it = lines.into_iter().peekable();
        while let Some(_) = it.peek() {
            entries.push(parse_entry(&mut it));
        }
        Ok(TaskpaperFile { entries })
    }

    pub fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    pub fn write(&self, path: impl AsRef<Path>, options: FormatOptions) -> Result<()> {
        let s = self.to_string(options);
        std::fs::write(path, s)?;
        Ok(())
    }

    /// Return all objects that match 'query'.
    pub fn search(&self, query: &str) -> Result<Vec<&Entry>> {
        fn recurse<'a>(entry: &'a Entry, expr: &search::Expr, out: &mut Vec<&'a Entry>) {
            match entry {
                Entry::Task(task) => {
                    if expr.evaluate(&task.tags).is_truish() {
                        out.push(entry);
                    }
                }
                Entry::Project(project) => {
                    if expr.evaluate(&project.tags).is_truish() {
                        out.push(entry);
                    }
                    for e in &project.children {
                        recurse(e, expr, out);
                    }
                }
                Entry::Note(_) => (),
            }
        }

        let expr = search::parse(query)?;
        let mut out = Vec::new();
        for e in &self.entries {
            recurse(e, &expr, &mut out);
        }
        Ok(out)
    }

    /// Removes all items from 'self' that match 'query' and return them in the returned value.
    /// If a parent item matches, the children are not tested further.
    pub fn filter(&mut self, query: &str) -> Result<Vec<Entry>> {
        fn recurse(
            entries: Vec<Entry>,
            expr: &search::Expr,
            filtered: &mut Vec<Entry>,
        ) -> Vec<Entry> {
            let mut retained = Vec::new();
            for e in entries {
                match e {
                    Entry::Task(t) => {
                        if expr.evaluate(&t.tags).is_truish() {
                            filtered.push(Entry::Task(t));
                        } else {
                            retained.push(Entry::Task(t));
                        }
                    }
                    Entry::Project(mut p) => {
                        if expr.evaluate(&p.tags).is_truish() {
                            filtered.push(Entry::Project(p));
                        } else {
                            p.children = recurse(p.children, expr, filtered);
                            retained.push(Entry::Project(p));
                        }
                    }
                    Entry::Note(n) => retained.push(Entry::Note(n)),
                }
            }
            retained
        }

        let expr = search::parse(query)?;
        let mut filtered = Vec::new();
        let mut entries = Vec::new();
        ::std::mem::swap(&mut self.entries, &mut entries);
        self.entries = recurse(entries, &expr, &mut filtered);
        Ok(filtered)
    }

    /// Finds the first project with the given name.
    pub fn get_project_mut(&mut self, text: &str) -> Option<&mut Project> {
        fn recurse<'a>(entry: &'a mut Entry, text: &str) -> Option<&'a mut Project> {
            match entry {
                Entry::Project(project) => {
                    if project.text == text {
                        return Some(project);
                    }
                    for e in &mut project.children {
                        if let Some(project) = recurse(e, text) {
                            return Some(project);
                        }
                    }
                }
                Entry::Task(_) | Entry::Note(_) => (),
            };
            None
        }

        for e in &mut self.entries {
            if let Some(child) = recurse(e, text) {
                return Some(child);
            }
        }
        None
    }
}

impl ToStringWithIndent for TaskpaperFile {
    fn append_to_string(&self, buf: &mut String, options: FormatOptions) -> fmt::Result {
        let entries: Vec<&Entry> = self.entries.iter().collect();
        &entries.append_to_string(buf, options)?;

        match options.vim_read_only {
            VimReadOnly::No => (),
            VimReadOnly::Yes => {
                buf.push_str("\n\nvim:ro\n");
            }
        }
        Ok(())
    }
}

fn find_tag(s: &str) -> Option<(Tag, (usize, usize))> {
    let mut tag_could_start = Some(0);
    let mut last_char = None;
    for (idx, c) in s.char_indices() {
        if let Some(sidx) = tag_could_start {
            if let Some(c) = TAGS.captures(&s[idx..]) {
                let name = c.get(1).unwrap().as_str()[1..].to_string(); // Remove @
                let value = c.get(2).map(|s| {
                    let s = s.as_str();
                    s[1..s.len() - 1].to_string() // Remove ()
                });
                let end = idx + c.get(0).unwrap().end();
                return Some((Tag { name, value }, (sidx, end)));
            }
        }
        tag_could_start = match c {
            ' ' | '(' | ')' => match last_char {
                Some(a) if a == c => tag_could_start,
                _ => Some(idx),
            },
            _ => None,
        };
        last_char = Some(c);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_find_tag() {
        fn check(input: &str, golden_tag: Tag, golden_consumed: usize) {
            let (tag, range) = find_tag(input).unwrap();
            assert_eq!(tag, golden_tag);
            let golden_range = (0, golden_consumed);
            assert_eq!(
                golden_range, range,
                "{} ({:?} != {:?})",
                input, golden_range, range
            );
        }
        check(
            "@done",
            Tag {
                name: "done".to_string(),
                value: None,
            },
            5,
        );
        check(
            "@due(today)",
            Tag {
                name: "due".to_string(),
                value: Some("today".to_string()),
            },
            11,
        );
        check(
            "@uuid(123-abc-ef)",
            Tag {
                name: "uuid".to_string(),
                value: Some("123-abc-ef".to_string()),
            },
            17,
        );
        check(
            "@another(foo bar)   ",
            Tag {
                name: "another".to_string(),
                value: Some("foo bar".to_string()),
            },
            17,
        );
        check(
            " @another(foo bar)   ",
            Tag {
                name: "another".to_string(),
                value: Some("foo bar".to_string()),
            },
            18,
        );
        check(
            "     @another(foo bar)",
            Tag {
                name: "another".to_string(),
                value: Some("foo bar".to_string()),
            },
            22,
        );
        check(
            "@foo @bar",
            Tag {
                name: "foo".to_string(),
                value: None,
            },
            4,
        );
    }

    #[test]
    fn test_extract_tag() {
        fn check(input: &str, num_tags: usize, golden_clean: &str) {
            let (clean, tags) = extract_tags(input);
            assert_eq!(golden_clean, clean);
            assert_eq!(num_tags, tags.len());
        }
        check("- foo blub @done", 1, "- foo blub");
        check("- foo @check blub @done @aaa", 3, "- foo blub");
    }

    #[test]
    fn test_simple_task_parse() {
        let input = r"- A task @tag1 @tag2";
        let golden = vec![Entry::Task(Task {
            text: "A task".to_string(),
            tags: {
                let mut tags = Tags::new();
                tags.insert(Tag {
                    name: "tag1".into(),
                    value: None,
                });
                tags.insert(Tag {
                    name: "tag2".into(),
                    value: None,
                });
                tags
            },
            note: None,
        })];
        let output = TaskpaperFile::parse(&input).unwrap();
        assert_eq!(golden, output.entries);
    }

    #[test]
    fn test_task_with_mixed_tags_parse() {
        let input = r"- A task @done(2018-08-05) @another(foo bar) @tag1 @tag2";
        let golden = vec![Entry::Task(Task {
            text: "A task".to_string(),
            tags: {
                let mut tags = Tags::new();
                tags.insert(Tag {
                    name: "tag1".into(),
                    value: None,
                });
                tags.insert(Tag {
                    name: "tag2".into(),
                    value: None,
                });
                tags.insert(Tag {
                    name: "done".into(),
                    value: Some("2018-08-05".into()),
                });
                tags.insert(Tag {
                    name: "another".into(),
                    value: Some("foo bar".into()),
                });
                tags
            },
            note: None,
        })];
        let output = TaskpaperFile::parse(&input).unwrap();
        assert_eq!(golden, output.entries);
    }

    #[test]
    fn test_parsing_roundtrip() {
        let input = include_str!("tests/simple_project_canonical_formatting.taskpaper");
        let tpf = TaskpaperFile::parse(&input).unwrap();
        assert_eq!(input, tpf.to_string(FormatOptions::default()));
    }

    #[test]
    fn test_reformatting_roundtrip() {
        let input = include_str!("tests/simple_project.taskpaper");
        let expected = include_str!("tests/simple_project_canonical_formatting.taskpaper");
        let tpf = TaskpaperFile::parse(&input).unwrap();
        assert_eq!(expected, tpf.to_string(FormatOptions::default()));
    }

    #[test]
    fn test_format_task() {
        let tpf = TaskpaperFile::parse(
            "- Arbeit • Foo • blah @blocked(arg prs) @coding @next @done(2018-06-21)",
        )
        .unwrap();
        let golden =
            "- Arbeit • Foo • blah @coding @next @blocked(arg prs) @done(2018-06-21)\n";
        assert_eq!(golden, tpf.to_string(FormatOptions::default()));
    }

    // NOCOM(#sirver): bring back
    #[test]
    fn test_simple_project_parse() {
        let input = include_str!("tests/simple_project.taskpaper");
        let output = TaskpaperFile::parse(&input).unwrap();
        let s = output.to_string(FormatOptions::default());
        // NOCOM(#sirver): test something with s?
        println!("{}", s);
        // TODO(sirver): This should use some diff algo to be easier to understand.
        assert_eq!(2, output.entries.len());

        // let golden = vec![
        // Entry::Project(Project {
        // text: "Project".into(),
        // tags: Tags::new(),
        // children: vec![
        // Entry::Task(Task {
        // text: "A task".to_string(),
        // tags: {
        // let mut tags = Tags::new();
        // tags.insert(Tag { name: "tag1".into(), value: None });
        // tags.insert(Tag { name: "tag2".into(), value: None });
        // tags
        // },
        // note: None
        // }),
        // ]
        // }),
        // ];
        // assert_eq!(golden, output);
    }
}
