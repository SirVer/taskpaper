pub mod db;
pub mod search;
pub mod tag;

// TODO(sirver): This should only be in cfg(test), but since it is used in the binary which is the
// only thing compiled with cfg test, it needs to be always included.
pub mod testing;

pub use crate::tag::{Tag, Tags};
pub use db::{CommonFileKind, Database};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt::{self, Write};
use std::io;
use std::iter::Peekable;
use std::mem;
use std::ops::{Index, IndexMut};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct NodeId(usize);

#[derive(Debug)]
pub struct Node {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    item: Item,
}

impl Node {
    pub fn item(&self) -> &Item {
        &self.item
    }

    pub fn item_mut(&mut self) -> &mut Item {
        &mut self.item
    }

    pub fn parent(&self) -> Option<&NodeId> {
        self.parent.as_ref()
    }
}

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
            })
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Project {
    pub line_index: Option<usize>,
    pub text: String,
    pub note: Option<Note>,
    pub tags: Tags,
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
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Item::Project(p) => &p.text,
            Item::Task(t) => &t.text,
        }
    }

    pub fn tags(&self) -> &Tags {
        match self {
            Item::Project(p) => &p.tags,
            Item::Task(t) => &t.tags,
        }
    }

    pub fn tags_mut(&mut self) -> &mut Tags {
        match self {
            Item::Project(p) => &mut p.tags,
            Item::Task(t) => &mut t.tags,
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

fn print_nodes(
    mut node_ids: Vec<NodeId>,
    arena: &[Node],
    buf: &mut String,
    indent: usize,
    options: FormatOptions,
) -> fmt::Result {
    // Projects are bubbled to the top.
    match options.sort {
        Sort::Nothing => (),
        Sort::ProjectsFirst => node_ids.sort_by_key(|id| !arena[id.0].item.is_project()),
    }

    let maybe_empty_line = |buf: &mut String, idx: usize| -> fmt::Result {
        // Only if there is a next item and that is a project do we actually print a new line.
        if let Some(id) = node_ids.get(idx + 1) {
            if arena[id.0].item.is_project() {
                writeln!(buf, "")?;
            }
        }
        Ok(())
    };

    for (idx, id) in node_ids.iter().enumerate() {
        let node = &arena[id.0];
        let add_empty_line = match &node.item {
            Item::Project(p) => {
                p.append_to_string(buf, indent, options)?;
                match indent {
                    0 => options.empty_line_after_project.top_level,
                    1 => options.empty_line_after_project.first_level,
                    _ => options.empty_line_after_project.others,
                }
            }
            Item::Task(t) => {
                t.append_to_string(buf, indent, options)?;
                0
            }
        };

        match options.print_children {
            PrintChildren::No => (),
            PrintChildren::Yes => {
                print_nodes(node.children.clone(), arena, buf, indent + 1, options)?
            }
        }

        for _ in 0..add_empty_line {
            maybe_empty_line(buf, idx)?;
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

fn parse_task(it: &mut Peekable<impl Iterator<Item = LineToken>>, arena: &mut Vec<Node>) -> NodeId {
    let token = it.next().unwrap();
    let (without_tags, tags) = tag::extract_tags(token.text);

    let note = match it.peek() {
        Some(nt) if nt.kind == LineKind::Note => Some(parse_note(it).text),
        _ => None,
    };

    let node_id = NodeId(arena.len());
    arena.push(Node {
        parent: None,
        children: Vec::new(),
        item: Item::Task(Task {
            line_index: Some(token.line_index),
            // Also trim the leading '- '
            text: without_tags.trim()[1..].trim_start().to_string(),
            tags,
            note,
        }),
    });
    node_id
}

fn parse_project(
    it: &mut Peekable<impl Iterator<Item = LineToken>>,
    arena: &mut Vec<Node>,
) -> NodeId {
    let token = it.next().unwrap();
    let (without_tags, tags) = tag::extract_tags(token.text);
    let without_tags = without_tags.trim();

    let note = match it.peek() {
        Some(nt) if nt.kind == LineKind::Note => Some(parse_note(it)),
        _ => None,
    };

    let node_id = NodeId(arena.len());
    arena.push(Node {
        parent: None,
        children: Vec::new(),
        item: Item::Project(Project {
            line_index: Some(token.line_index),
            // Also trim the trailing ':'
            text: without_tags[..without_tags.len() - 1].to_string(),
            note,
            tags,
        }),
    });

    let mut children = Vec::new();
    while let Some(nt) = it.peek() {
        if nt.indent <= token.indent {
            break;
        }
        let child_node = parse_item(it, arena);
        arena[child_node.0].parent = Some(node_id.clone());
        children.push(child_node);
    }
    arena[node_id.0].children = children;
    node_id
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

fn parse_item(it: &mut Peekable<impl Iterator<Item = LineToken>>, arena: &mut Vec<Node>) -> NodeId {
    let token = it.peek().unwrap();
    match token.kind {
        LineKind::Task => parse_task(it, arena),
        LineKind::Project => parse_project(it, arena),
        // TODO(sirver): This must absolutely not panic.
        LineKind::Note => panic!("Notes only supported as first children of tasks and projects."),
    }
}

#[derive(Debug)]
pub struct TaskpaperFile {
    arena: Vec<Node>,
    nodes: Vec<NodeId>,

    /// If this was loaded from a file, this will be set to the path of that file.
    path: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub enum Position {
    AsFirst,
    AsLast,
}

#[derive(Clone, Copy)]
pub enum Level<'a> {
    Top,
    Under(&'a NodeId),
}

impl TaskpaperFile {
    pub fn new() -> Self {
        TaskpaperFile {
            arena: Vec::new(),
            nodes: Vec::new(),
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

        let mut nodes = Vec::new();
        let mut arena = Vec::new();

        let mut it = lines.into_iter().peekable();
        while let Some(_) = it.peek() {
            nodes.push(parse_item(&mut it, &mut arena));
        }
        Ok(TaskpaperFile {
            arena,
            nodes,
            path: None,
        })
    }

    fn register_item(&mut self, item: Item) -> NodeId {
        self.arena.push(Node {
            parent: None,
            children: Vec::new(),
            item: sanitize(item),
        });
        NodeId(self.arena.len() - 1)
    }

    pub fn sort_nodes_by_key<K, F>(&mut self, mut f: F)
    where
        F: FnMut(&Node) -> K,
        K: Ord,
    {
        let mut nodes = mem::replace(&mut self.nodes, Vec::new());
        nodes.sort_by_key(|id| f(&self.arena[id.0]));
        self.nodes = nodes;
    }

    pub fn insert(&mut self, item: Item, level: Level, position: Position) -> NodeId {
        let node_id = self.register_item(item);
        self.insert_node(node_id.clone(), level, position);
        node_id
    }

    pub fn insert_node(&mut self, node_id: NodeId, level: Level, position: Position) {
        let vec = match level {
            Level::Top => &mut self.nodes,
            Level::Under(id) => &mut self.arena[id.0].children,
        };
        match position {
            Position::AsFirst => vec.insert(0, node_id.clone()),
            Position::AsLast => vec.push(node_id.clone()),
        }
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

    /// Return all objects that match 'query' in order of appearance in the file.
    pub fn search(&self, query: &str) -> Result<Vec<NodeId>> {
        let expr = search::Expr::parse(query)?;
        let mut out = Vec::new();
        for node in self {
            let tags = match node.item() {
                Item::Task(task) => &task.tags,
                Item::Project(project) => &project.tags,
            };
            if expr.evaluate(tags).is_truish() {
                out.push(node.id().clone());
            }
        }
        Ok(out)
    }

    /// Removes all items from 'self' that match 'query' and return them in the returned value.
    /// If a parent item matches, the children are not tested further.
    pub fn filter(&mut self, query: &str) -> Result<Vec<NodeId>> {
        fn recurse(
            arena: &mut [Node],
            node_ids: Vec<NodeId>,
            expr: &search::Expr,
            filtered: &mut Vec<NodeId>,
        ) -> Vec<NodeId> {
            let mut retained = Vec::new();
            for node_id in node_ids {
                let tags = match arena[node_id.0].item() {
                    Item::Task(t) => &t.tags,
                    Item::Project(p) => &p.tags,
                };

                if expr.evaluate(&tags).is_truish() {
                    filtered.push(node_id);
                } else {
                    retained.push(node_id.clone());
                    let children = mem::replace(&mut arena[node_id.0].children, Vec::new());
                    arena[node_id.0].children = recurse(arena, children, expr, filtered);
                }
            }
            retained
        }

        let expr = search::Expr::parse(query)?;
        let mut filtered = Vec::new();
        let nodes = mem::replace(&mut self.nodes, Vec::new());
        self.nodes = recurse(&mut self.arena, nodes, &expr, &mut filtered);
        Ok(filtered)
    }

    /// Copy the node with 'source_id' from 'source' into us, including its entry and all sub
    /// nodes. Does not link it into the file tree, this needs to be done later manually.
    pub fn copy_node(&mut self, source: &TaskpaperFile, source_id: &NodeId) -> NodeId {
        fn recurse(arena: &mut Vec<Node>, source: &TaskpaperFile, source_id: &NodeId) -> NodeId {
            let id = NodeId(arena.len());
            let source_node = &source.arena[source_id.0];
            arena.push(Node {
                parent: None,
                item: source_node.item().clone(),
                children: Vec::new(),
            });
            let mut children = Vec::with_capacity(source_node.children.len());
            for child_id in &source_node.children {
                children.push(recurse(arena, source, child_id));
            }
            arena[id.0].children = children;
            id
        }
        recurse(&mut self.arena, source, source_id)
    }

    pub fn iter(&self) -> TaskpaperIter {
        TaskpaperIter {
            tpf: self,
            open: self.nodes.iter().cloned().collect(),
        }
    }

    pub fn iter_mut(&mut self) -> TaskpaperIterMut {
        let open = self.nodes.iter().cloned().collect();
        TaskpaperIterMut { tpf: self, open }
    }

    /// Removes the node with the given 'node_id' from the File, i.e. unlinks it from its parent.
    pub fn unlink_node(&mut self, node_id: NodeId) {
        if self.arena[node_id.0].parent().is_some() {
            let parent_id = self.arena[node_id.0].parent().unwrap().0;
            let parent_node = &mut self.arena[parent_id];
            let pos = parent_node
                .children
                .iter()
                .position(|x| x.0 == node_id.0)
                .expect("The parent of a node does not have this node as child.");
            parent_node.children.remove(pos);
        } else {
            let pos = self
                .nodes
                .iter()
                .position(|x| x.0 == node_id.0)
                .expect("The parent of a node does not have this node as child.");
            self.nodes.remove(pos);
        }
        self.arena[node_id.0].parent = None;
    }
}

impl<'a> Index<&'a NodeId> for TaskpaperFile {
    type Output = Node;

    fn index(&self, node_id: &'a NodeId) -> &Self::Output {
        &self.arena[node_id.0]
    }
}

impl<'a> IndexMut<&'a NodeId> for TaskpaperFile {
    fn index_mut(&mut self, node_id: &'a NodeId) -> &mut Self::Output {
        &mut self.arena[node_id.0]
    }
}

// TODO(sirver): IterItem and IterItemMut seem unnecessary, they are essentially Nodes.
#[derive(Debug)]
pub struct IterItem<'a> {
    node: &'a Node,
    node_id: NodeId,
}

impl<'a> IterItem<'a> {
    pub fn item(&'a self) -> &'a Item {
        &self.node.item
    }

    pub fn id(&self) -> &NodeId {
        &self.node_id
    }
}

pub struct TaskpaperIter<'a> {
    tpf: &'a TaskpaperFile,
    open: VecDeque<NodeId>,
}

impl<'a> Iterator for TaskpaperIter<'a> {
    type Item = IterItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let node_id = match self.open.pop_front() {
            None => return None,
            Some(id) => id,
        };
        let node = &self.tpf.arena[node_id.0];
        for child_id in node.children.iter().rev() {
            self.open.push_front(child_id.clone());
        }
        Some(IterItem { node, node_id })
    }
}

impl<'a> IntoIterator for &'a TaskpaperFile {
    type IntoIter = TaskpaperIter<'a>;
    type Item = IterItem<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug)]
pub struct IterMutItem {
    node: *mut Node,
    node_id: NodeId,
}

impl IterMutItem {
    pub fn item_mut(&mut self) -> &mut Item {
        // Safe by construction:
        // The iterator holds a ref onto the TaskpaperFile, which makes it illegal
        // to add new items - this guarantees that this pointer is still valid while
        // the iterator is alive.
        // IterMutItem guards the mutability of the underlying node: The topology of the file
        // cannot be changed, but the Item it is pointing to can be freely be modified without
        // actually changing the TaskpaperFileStruct.
        unsafe { &mut (*self.node).item }
    }

    pub fn item(&self) -> &Item {
        // See 'item_mut'.
        unsafe { &(*self.node).item }
    }

    pub fn id(&self) -> &NodeId {
        &self.node_id
    }
}

pub struct TaskpaperIterMut<'a> {
    tpf: &'a mut TaskpaperFile,
    open: VecDeque<NodeId>,
}

impl<'a> Iterator for TaskpaperIterMut<'a> {
    type Item = IterMutItem;

    fn next(&mut self) -> Option<Self::Item> {
        let node_id = match self.open.pop_front() {
            None => return None,
            Some(node_id) => node_id,
        };
        for child_id in self.tpf.arena[node_id.0].children.iter().rev() {
            self.open.push_front(child_id.clone());
        }
        Some(IterMutItem {
            node: &mut self.tpf.arena[node_id.0],
            node_id,
        })
    }
}

impl<'a> IntoIterator for &'a mut TaskpaperFile {
    type Item = IterMutItem;
    type IntoIter = TaskpaperIterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl ToStringWithIndent for TaskpaperFile {
    fn append_to_string(
        &self,
        buf: &mut String,
        indent: usize,
        options: FormatOptions,
    ) -> fmt::Result {
        print_nodes(self.nodes.clone(), &self.arena, buf, indent, options)
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
    for mut dest_node in destination {
        for source_node in &source {
            if source_node.item().text() != dest_node.item().text() {
                continue;
            }

            match (&source_node.item(), dest_node.item_mut()) {
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
            break; // We only use the first matching item.
        }
    }

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
        let items: Vec<Item> = output.iter().map(|n| n.item().clone()).collect();
        assert_eq!(golden, items);
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
        let items: Vec<Item> = output.iter().map(|n| n.item().clone()).collect();
        assert_eq!(golden, items);
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
