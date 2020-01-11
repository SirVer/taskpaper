pub mod db;
pub mod search;
pub mod tag;

// TODO(sirver): This should only be in cfg(test), but since it is used in the binary which is the
// only thing compiled with cfg test, it needs to be always included.
pub mod testing;

pub use crate::tag::{Tag, Tags};
pub use db::{CommonFileKind, Database};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};

// TODO(sirver): Convert to use thiserror
#[derive(Debug)]
pub enum Error {
    Misc(String),
    Other(Box<dyn ::std::error::Error>),
    Io(io::Error),
    QuerySyntaxError(String),
}

impl From<io::Error> for Error {
    fn from(other: io::Error) -> Error {
        Error::Io(other)
    }
}

impl From<Box<dyn ::std::error::Error>> for Error {
    fn from(other: Box<dyn ::std::error::Error>) -> Error {
        Error::Other(other)
    }
}

impl Error {
    pub fn misc(text: impl Into<String>) -> Self {
        Error::Misc(text.into())
    }
}

pub type Result<T> = ::std::result::Result<T, Error>;

fn sanitize(item: Item) -> Item {
    // Make sure the line does not contain a newline and does not end with ':'
    fn sanitize_note(s: Option<String>) -> Option<String> {
        match s {
            None => None,
            Some(s) => {
                let t = s
                    .split("\n")
                    .map(|l| l.trim_end().trim_end_matches(':'))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            }
        }
    }

    // Make sure none of the note texts end with ':'.
    fn sanitize_text(s: String) -> String {
        s.replace('\n', " ").trim_end_matches(':').to_string()
    }

    match item {
        Item::Task(t) => Item::Task(Task {
            tags: t.tags,
            text: sanitize_text(t.text),
            note: sanitize_note(t.note),
            line_index: t.line_index,
        }),
        Item::Project(p) => {
            let note = match p.note {
                None => None,
                Some(n) => {
                    let new_text = sanitize_note(Some(n.text));
                    new_text.map(|text| Note { text })
                }
            };
            Item::Project(Project {
                line_index: p.line_index,
                text: sanitize_text(p.text),
                note,
                tags: p.tags,
                children: p.children.into_iter().map(|e| sanitize(e)).collect(),
            })
        }
        Item::Note(n) => Item::Note(n),
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Project {
    pub line_index: Option<usize>,
    pub text: String,
    pub note: Option<Note>,
    pub tags: Tags,
    pub children: Vec<Item>,
}

impl Project {
    pub fn push_back(&mut self, item: Item) {
        self.children.push(sanitize(item));
    }

    pub fn push_front(&mut self, item: Item) {
        self.children.insert(0, sanitize(item));
    }
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
    // TODO(sirver): Document.
    pub top_level: usize,
    pub first_level: usize,
    pub others: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PrintChildren {
    // TODO(sirver): Document.
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PrintNotes {
    // TODO(sirver): Document.
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FormatOptions {
    pub sort: Sort,
    pub print_children: PrintChildren,
    pub print_notes: PrintNotes,
    pub empty_line_after_project: EmptyLineAfterProject,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            sort: Sort::ProjectsFirst,
            print_children: PrintChildren::Yes,
            print_notes: PrintNotes::Yes,
            empty_line_after_project: EmptyLineAfterProject {
                top_level: 1,
                first_level: 1,
                others: 0,
            },
        }
    }
}

impl ToStringWithIndent for Project {
    fn append_to_string(
        &self,
        buf: &mut String,
        indent: usize,
        options: FormatOptions,
    ) -> fmt::Result {
        let indent_str = "\t".repeat(indent);
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
                    let note_indent = "\t".repeat(indent + 1);
                    for line in note.text.split_terminator('\n') {
                        writeln!(buf, "{}{}", note_indent, line)?;
                    }
                }
            }
        }

        match options.print_children {
            PrintChildren::No => (),
            PrintChildren::Yes => {
                print_items(buf, self.children.iter().collect(), indent + 1, options)?
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Task {
    pub tags: Tags,
    pub text: String,
    // TODO(sirver): Consider to use Note here instead of String.
    pub note: Option<String>,
    pub line_index: Option<usize>,
}

pub trait ToStringWithIndent {
    fn append_to_string(
        &self,
        buf: &mut String,
        indent: usize,
        options: FormatOptions,
    ) -> fmt::Result;

    fn to_string(&self, indent: usize, options: FormatOptions) -> String {
        let mut s = String::new();
        self.append_to_string(&mut s, indent, options).unwrap();
        s
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Note {
    pub text: String,
}

impl ToStringWithIndent for Note {
    fn append_to_string(&self, buf: &mut String, indent: usize, _: FormatOptions) -> fmt::Result {
        let indent = "\t".repeat(indent);
        for line in self.text.split_terminator('\n') {
            writeln!(buf, "{}{}", indent, line)?;
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Item {
    Task(Task),
    Project(Project),
    Note(Note),
}

impl Item {
    pub fn is_project(&self) -> bool {
        match *self {
            Item::Project(_) => true,
            _ => false,
        }
    }

    pub fn line_index(&self) -> Option<usize> {
        match self {
            Item::Project(p) => p.line_index,
            Item::Task(t) => t.line_index,
            Item::Note(_) => None,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Item::Note(n) => &n.text,
            Item::Project(p) => &p.text,
            Item::Task(t) => &t.text,
        }
    }
}

impl ToStringWithIndent for Item {
    fn append_to_string(
        &self,
        buf: &mut String,
        indent: usize,
        options: FormatOptions,
    ) -> fmt::Result {
        match self {
            Item::Task(t) => t.append_to_string(buf, indent, options),
            Item::Project(p) => p.append_to_string(buf, indent, options),
            Item::Note(n) => n.append_to_string(buf, indent, options),
        }
    }
}

impl ToStringWithIndent for Task {
    fn append_to_string(
        &self,
        buf: &mut String,
        indent: usize,
        options: FormatOptions,
    ) -> fmt::Result {
        let indent_str = "\t".repeat(indent);
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
                    let note_indent = "\t".repeat(indent + 1);
                    for line in note.split_terminator('\n') {
                        writeln!(buf, "{}{}", note_indent, line)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl ToStringWithIndent for [&Item] {
    fn append_to_string(
        &self,
        buf: &mut String,
        indent: usize,
        options: FormatOptions,
    ) -> fmt::Result {
        print_items(buf, self.to_vec(), indent, options)?;
        Ok(())
    }
}

fn print_items(
    buf: &mut String,
    mut items: Vec<&Item>,
    indent: usize,
    options: FormatOptions,
) -> fmt::Result {
    // Projects are bubbled to the top.
    match options.sort {
        Sort::Nothing => (),
        Sort::ProjectsFirst => items.sort_by_key(|a| !a.is_project()),
    }

    let maybe_empty_line = |buf: &mut String, idx: usize| -> fmt::Result {
        // Only if there is a next item and that is a project do we actually print a new line.
        if let Some(s) = items.get(idx + 1) {
            if s.is_project() {
                writeln!(buf, "")?;
            }
        }
        Ok(())
    };

    for (idx, e) in items.iter().enumerate() {
        match e {
            Item::Project(p) => {
                p.append_to_string(buf, indent, options)?;
                let add_empty_line = match indent {
                    0 => options.empty_line_after_project.top_level,
                    1 => options.empty_line_after_project.first_level,
                    _ => options.empty_line_after_project.others,
                };
                for _ in 0..add_empty_line {
                    maybe_empty_line(buf, idx)?;
                }
            }
            Item::Task(t) => {
                t.append_to_string(buf, indent, options)?;
            }
            Item::Note(n) => n.append_to_string(buf, indent, options)?,
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
    line.trim_start().starts_with("- ")
}

fn indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == '\t').count()
}

fn is_project(line: &str) -> bool {
    line.trim_end().ends_with(':')
}

fn classify(line: &str) -> LineKind {
    let (without_tags, _) = tag::extract_tags(line.to_string());
    if is_task(&without_tags) {
        LineKind::Task
    } else if is_project(&without_tags) {
        LineKind::Project
    } else {
        LineKind::Note
    }
}

#[derive(Debug)]
struct LineToken {
    line_index: usize,
    indent: usize,
    text: String,
    kind: LineKind,
}

fn parse_task(it: &mut Peekable<impl Iterator<Item = LineToken>>) -> Task {
    let token = it.next().unwrap();
    let (without_tags, tags) = tag::extract_tags(token.text);

    let note = match it.peek() {
        Some(nt) if nt.kind == LineKind::Note => Some(parse_note(it).text),
        _ => None,
    };

    Task {
        line_index: Some(token.line_index),
        // Also trim the leading '- '
        text: without_tags.trim()[1..].trim_start().to_string(),
        tags,
        note,
    }
}

fn parse_project(it: &mut Peekable<impl Iterator<Item = LineToken>>) -> Project {
    let token = it.next().unwrap();
    let (without_tags, tags) = tag::extract_tags(token.text);
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
        children.push(parse_item(it));
    }

    Project {
        line_index: Some(token.line_index),
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

fn parse_item(it: &mut Peekable<impl Iterator<Item = LineToken>>) -> Item {
    let token = it.peek().unwrap();
    match token.kind {
        LineKind::Task => Item::Task(parse_task(it)),
        LineKind::Project => Item::Project(parse_project(it)),
        LineKind::Note => Item::Note(parse_note(it)),
    }
}

#[derive(Debug)]
pub struct TaskpaperFile {
    pub items: Vec<Item>,

    /// If this was loaded from a file, this will be set to the path of that file.
    path: Option<PathBuf>,
}

impl TaskpaperFile {
    pub fn new() -> Self {
        TaskpaperFile {
            items: Vec::new(),
            path: None,
        }
    }

    pub fn parse_file(path: impl AsRef<Path>) -> Result<Self> {
        let text = ::std::fs::read_to_string(&path)?;
        let mut s = Self::parse(&text)?;
        s.path = Some(path.as_ref().to_path_buf());
        Ok(s)
    }

    pub fn parse(input: &str) -> Result<Self> {
        let lines = input
            .trim()
            .lines()
            .enumerate()
            .filter(|(_line_index, line)| !line.trim().is_empty())
            .map(|(line_index, line)| LineToken {
                line_index: line_index,
                indent: indent(line),
                kind: classify(line),
                text: line.trim().to_string(),
            });

        let mut items = Vec::new();
        let mut it = lines.into_iter().peekable();
        while let Some(_) = it.peek() {
            items.push(parse_item(&mut it));
        }
        Ok(TaskpaperFile { items, path: None })
    }

    pub fn push_back(&mut self, item: Item) {
        self.items.push(sanitize(item));
    }

    pub fn push_front(&mut self, item: Item) {
        self.items.insert(0, sanitize(item));
    }

    pub fn write(&self, path: impl AsRef<Path>, options: FormatOptions) -> Result<()> {
        let new = self.to_string(0, options);

        let has_changed = match std::fs::read_to_string(&path) {
            Err(_) => true,
            Ok(old) => sha1::Sha1::from(&old) != sha1::Sha1::from(&new),
        };

        if has_changed {
            std::fs::write(&path, new)?;
        }
        Ok(())
    }

    /// Return all objects that match 'query'.
    pub fn search(&self, query: &str) -> Result<Vec<&Item>> {
        fn recurse<'a>(item: &'a Item, expr: &search::Expr, out: &mut Vec<&'a Item>) {
            match item {
                Item::Task(task) => {
                    if expr.evaluate(&task.tags).is_truish() {
                        out.push(item);
                    }
                }
                Item::Project(project) => {
                    if expr.evaluate(&project.tags).is_truish() {
                        out.push(item);
                    }
                    for e in &project.children {
                        recurse(e, expr, out);
                    }
                }
                Item::Note(_) => (),
            }
        }

        let expr = search::Expr::parse(query)?;
        let mut out = Vec::new();
        for e in &self.items {
            recurse(e, &expr, &mut out);
        }
        Ok(out)
    }

    /// Find all items with exactly the given text.
    pub fn get_items(&self, text: &str) -> Vec<&Item> {
        let mut result = Vec::new();
        self.map(|e| {
            if e.text() == text {
                result.push(e as &Item);
            }
        });
        result
    }

    /// Removes all items from 'self' that match 'query' and return them in the returned value.
    /// If a parent item matches, the children are not tested further.
    pub fn filter(&mut self, query: &str) -> Result<Vec<Item>> {
        fn recurse(items: Vec<Item>, expr: &search::Expr, filtered: &mut Vec<Item>) -> Vec<Item> {
            let mut retained = Vec::new();
            for e in items {
                match e {
                    Item::Task(t) => {
                        if expr.evaluate(&t.tags).is_truish() {
                            filtered.push(Item::Task(t));
                        } else {
                            retained.push(Item::Task(t));
                        }
                    }
                    Item::Project(mut p) => {
                        if expr.evaluate(&p.tags).is_truish() {
                            filtered.push(Item::Project(p));
                        } else {
                            p.children = recurse(p.children, expr, filtered);
                            retained.push(Item::Project(p));
                        }
                    }
                    Item::Note(n) => retained.push(Item::Note(n)),
                }
            }
            retained
        }

        let expr = search::Expr::parse(query)?;
        let mut filtered = Vec::new();
        let mut items = Vec::new();
        ::std::mem::swap(&mut self.items, &mut items);
        self.items = recurse(items, &expr, &mut filtered);
        Ok(filtered)
    }

    /// Finds the first project with the given name.
    pub fn get_project_mut(&mut self, text: &str) -> Option<&mut Project> {
        fn recurse<'a>(item: &'a mut Item, text: &str) -> Option<&'a mut Project> {
            match item {
                Item::Project(project) => {
                    if project.text == text {
                        return Some(project);
                    }
                    for e in &mut project.children {
                        if let Some(project) = recurse(e, text) {
                            return Some(project);
                        }
                    }
                }
                Item::Task(_) | Item::Note(_) => (),
            };
            None
        }

        for e in &mut self.items {
            if let Some(child) = recurse(e, text) {
                return Some(child);
            }
        }
        None
    }

    /// Call `f` on all items in this file in order of appearance in the file, including all
    /// children of projects.
    pub fn map_mut(&mut self, mut f: impl Fn(&mut Item)) {
        fn recurse(items: &mut [Item], f: &mut impl FnMut(&mut Item)) {
            for e in items.iter_mut() {
                f(e);
                match e {
                    Item::Project(ref mut p) => {
                        recurse(&mut p.children, f);
                    }
                    _ => (),
                }
            }
        }
        recurse(&mut self.items, &mut f);
    }

    pub fn map<'a>(&'a self, mut f: impl FnMut(&'a Item)) {
        fn recurse<'b>(items: &'b [Item], f: &mut impl FnMut(&'b Item)) {
            for e in items.iter() {
                f(e);
                match e {
                    Item::Project(ref p) => {
                        recurse(&p.children, f);
                    }
                    _ => (),
                }
            }
        }
        recurse(&self.items, &mut f);
    }
}

impl ToStringWithIndent for TaskpaperFile {
    fn append_to_string(
        &self,
        buf: &mut String,
        indent: usize,
        options: FormatOptions,
    ) -> fmt::Result {
        let items: Vec<&Item> = self.items.iter().collect();
        &items.append_to_string(buf, indent, options)?;

        Ok(())
    }
}

pub fn mirror_changes(
    source_path: impl AsRef<Path>,
    destination: &mut TaskpaperFile,
) -> Result<()> {
    if let Some(destination_path) = &destination.path {
        let source_path = source_path.as_ref();
        let source_changed = source_path.metadata()?.modified()?;
        let destination_changed = destination_path.metadata()?.modified()?;
        if destination_changed >= source_changed {
            // Destination is newer than the source. We better not blindly overwrite.
            return Ok(());
        }
    }

    let source = TaskpaperFile::parse_file(source_path)?;
    destination.map_mut(|e| {
        let items = source.get_items(e.text());
        if items.is_empty() {
            return;
        }

        match (&items[0], e) {
            (Item::Note(s), Item::Note(d)) => d.text = s.text.clone(),
            (Item::Project(s), Item::Project(d)) => {
                d.text = s.text.clone();
                d.tags = s.tags.clone();
                if s.note.is_some() {
                    d.note = s.note.clone();
                }
            }
            (Item::Task(s), Item::Task(d)) => {
                d.text = s.text.clone();
                d.tags = s.tags.clone();
                if s.note.is_some() {
                    d.note = s.note.clone();
                }
            }
            _ => (),
        };
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_simple_task_parse() {
        let input = r"- A task @tag1 @tag2";
        let golden = vec![Item::Task(Task {
            line_index: Some(0),
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
        assert_eq!(golden, output.items);
    }

    #[test]
    fn test_task_with_mixed_tags_parse() {
        let input = r"- A task @done(2018-08-05) @another(foo bar) @tag1 @tag2";
        let golden = vec![Item::Task(Task {
            text: "A task".to_string(),
            line_index: Some(0),
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
        assert_eq!(golden, output.items);
    }

    #[test]
    fn test_parsing_roundtrip() {
        let input = include_str!("tests/simple_project_canonical_formatting.taskpaper");
        let tpf = TaskpaperFile::parse(&input).unwrap();
        assert_eq!(input, tpf.to_string(0, FormatOptions::default()));
    }

    #[test]
    fn test_reformatting_roundtrip() {
        let input = include_str!("tests/simple_project.taskpaper");
        let expected = include_str!("tests/simple_project_canonical_formatting.taskpaper");
        let tpf = TaskpaperFile::parse(&input).unwrap();
        assert_eq!(expected, tpf.to_string(0, FormatOptions::default()));
    }

    #[test]
    fn test_format_task() {
        let tpf = TaskpaperFile::parse(
            "- Arbeit • Foo • blah @blocked(arg prs) @coding @next @done(2018-06-21)",
        )
        .unwrap();
        let golden = "- Arbeit • Foo • blah @coding @next @blocked(arg prs) @done(2018-06-21)\n";
        assert_eq!(golden, tpf.to_string(0, FormatOptions::default()));
    }

    #[test]
    fn test_mirror_changes_nothing_happens_when_destination_is_newer() {
        let test = DatabaseTest::new();
        let source = test.write_file(
            "source.taskpaper",
            include_str!("tests/mirror_changes/source.taskpaper"),
        );
        let destination_path = test.write_file(
            "destination.taskpaper",
            include_str!("tests/mirror_changes/destination.taskpaper"),
        );
        let mut destination = TaskpaperFile::parse_file(&destination_path).unwrap();
        mirror_changes(&source, &mut destination).expect("Should work.");
        assert_eq!(
            &destination.to_string(0, FormatOptions::default()),
            include_str!("tests/mirror_changes/destination.taskpaper"),
        );
    }

    #[test]
    fn test_mirror_changes() {
        let test = DatabaseTest::new();
        let mut destination =
            TaskpaperFile::parse(include_str!("tests/mirror_changes/destination.taskpaper"))
                .unwrap();
        let source = test.write_file(
            "source.taskpaper",
            include_str!("tests/mirror_changes/source.taskpaper"),
        );
        mirror_changes(&source, &mut destination).expect("Should work");
        assert_eq!(
            &destination.to_string(0, FormatOptions::default()),
            include_str!("tests/mirror_changes/destination_golden.taskpaper"),
        );
    }
}
