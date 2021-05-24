pub mod db;
pub mod search;
pub mod tag;

// TODO(sirver): This should only be in cfg(test), but since it is used in the binary which is the
// only thing compiled with cfg test, it needs to be always included.
pub mod testing;

pub use crate::tag::{Tag, Tags};
pub use db::{CommonFileKind, Database};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp;
use std::collections::VecDeque;
use std::fmt::{self, Write};
use std::io;
use std::iter::Peekable;
use std::mem;
use std::ops::{Index, IndexMut};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeId(usize);

impl NodeId {
    #[cfg(feature = "bindings")]
    pub fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    #[cfg(feature = "bindings")]
    pub fn from_u64(id: u64) -> Self {
        NodeId(id as usize)
    }
}

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

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] io::Error),

    #[error("invalid query: {0}")]
    QuerySyntaxError(String),
}

pub type Result<T> = ::std::result::Result<T, Error>;

/// Takes some 'text' in and returns a string that is valid for an item. This will turn all
/// whitespace into space, remove trailing : and leading '- '.
pub fn sanitize_item_text(text: &str) -> String {
    // Make sure the line does not contain a newline and does not end with ':'
    text.replace(|c| c == '\t' || c == '\n' || c == '\r', " ")
        .trim()
        .trim_end_matches(':')
        .trim_start_matches("- ")
        .to_string()
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
pub struct FormatOptions {
    pub sort: Sort,
    pub empty_line_after_project: EmptyLineAfterProject,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            sort: Sort::ProjectsFirst,
            empty_line_after_project: EmptyLineAfterProject {
                top_level: 1,
                first_level: 1,
                others: 0,
            },
        }
    }
}

fn append_project_to_string(item: &Item, buf: &mut String, indent: usize) -> fmt::Result {
    let indent_str = "\t".repeat(indent);
    let mut tags = item.tags.iter().map(|t| t.to_string()).collect::<Vec<_>>();
    tags.sort();
    let tags_string = if tags.is_empty() {
        "".to_string()
    } else {
        format!(" {}", tags.join(" "))
    };
    writeln!(buf, "{}{}:{}", indent_str, item.text, tags_string)?;

    Ok(())
}

fn append_note_to_string(item: &Item, buf: &mut String, indent: usize) -> fmt::Result {
    let indent = "\t".repeat(indent);
    for line in item.text.split_terminator('\n') {
        writeln!(buf, "{}{}", indent, line)?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ItemKind {
    Project,
    Task,
    Note,
}

// TODO(sirver): The goal should be to keep the contents of files unchanged as much as possible.
// The current layout of the Item struct does not make this possible.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Item {
    pub kind: ItemKind,

    /// The text of this item with any tags stripped and leading indent stripped. It is guaranteed
    /// that this text does not neither contain a newline '\n' or a carriage return '\r' character.
    pub text: String,

    /// The collection of Tags that this item contains. Order of the tags is currently lost,
    /// they will be reordered on write.
    pub tags: Tags,
    line_index: Option<usize>,

    /// The indentation level of this item. Since it holds that indent(child) >= indent(parent) + 1, the
    /// indentation is not implicit, but can indeed be different for every child. This can be 0 for
    /// new items, the items will be indented when they get a parent assigned.
    pub indent: u32,
}

impl Item {
    pub fn new(kind: ItemKind, text: String) -> Self {
        assert!(
            text.find('\r').is_none(),
            "Item text {} contains '\\r'",
            text
        );
        assert!(
            text.find('\n').is_none(),
            "Item text {} contains '\\n'",
            text
        );

        Item {
            kind,
            text,
            tags: Tags::new(),
            line_index: None,
            indent: 0,
        }
    }

    pub fn new_with_tags(kind: ItemKind, text: String, tags: Tags) -> Self {
        let mut item = Self::new(kind, text);
        item.tags = tags;
        item
    }
}

impl Item {
    pub fn is_task(&self) -> bool {
        match &self.kind {
            ItemKind::Task => true,
            _ => false,
        }
    }

    pub fn is_note(&self) -> bool {
        match &self.kind {
            ItemKind::Note => true,
            _ => false,
        }
    }

    pub fn is_project(&self) -> bool {
        match &self.kind {
            ItemKind::Project => true,
            _ => false,
        }
    }

    pub fn line_index(&self) -> Option<usize> {
        // TODO(sirver): return by ref
        self.line_index
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tags(&self) -> &Tags {
        &self.tags
    }

    pub fn tags_mut(&mut self) -> &mut Tags {
        &mut self.tags
    }
}

fn append_task_to_string(item: &Item, buf: &mut String, indent: usize) -> fmt::Result {
    let indent_str = "\t".repeat(indent);
    let mut tags = item.tags.iter().collect::<Vec<Tag>>();
    tags.sort_by_key(|t| (t.value.is_some(), t.name.clone()));
    let tags_string = if tags.is_empty() {
        "".to_string()
    } else {
        let tag_strings = tags.iter().map(|t| t.to_string()).collect::<Vec<String>>();
        format!(" {}", tag_strings.join(" "))
    };
    writeln!(buf, "{}- {}{}", indent_str, item.text, tags_string)?;
    Ok(())
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
                writeln!(buf)?;
            }
        }
        Ok(())
    };

    for (idx, id) in node_ids.iter().enumerate() {
        let node = &arena[id.0];
        let add_empty_line = match &node.item.kind {
            ItemKind::Project => {
                append_project_to_string(&node.item, buf, indent)?;
                match indent {
                    0 => options.empty_line_after_project.top_level,
                    1 => options.empty_line_after_project.first_level,
                    _ => options.empty_line_after_project.others,
                }
            }
            ItemKind::Task => {
                append_task_to_string(&node.item, buf, indent)?;
                0
            }
            ItemKind::Note => {
                append_note_to_string(&node.item, buf, indent)?;
                0
            }
        };

        print_nodes(node.children.clone(), arena, buf, indent + 1, options)?;

        for _ in 0..add_empty_line {
            maybe_empty_line(buf, idx)?;
        }
    }
    Ok(())
}

// This is the same as ItemKind at the moment, but I believe dealing with empty lines is easier if
// this is kept separate.
#[derive(Debug, PartialEq)]
enum LineKind {
    Task,
    Project,
    Note,
}

fn is_task(line: &str) -> bool {
    line.trim_start().starts_with("- ")
}

fn find_indent(line: &str) -> u32 {
    line.chars().take_while(|c| *c == '\t').count() as u32
}

fn is_project(line: &str) -> bool {
    line.trim_end().ends_with(':')
}

fn classify(without_tags: &str) -> LineKind {
    if is_task(&without_tags) {
        LineKind::Task
    } else if is_project(&without_tags) {
        LineKind::Project
    } else {
        LineKind::Note
    }
}

fn parse_task_text(line_without_tags: &str) -> String {
    // Trim the leading '- '
    line_without_tags.trim()[1..].trim_start().to_string()
}

fn parse_project_text(line_without_tags: &str) -> String {
    let without_tags = line_without_tags.trim();
    // Trim the trailing ':'
    without_tags[..without_tags.len() - 1].to_string()
}

fn parse_item<'a>(
    it: &mut Peekable<impl Iterator<Item = (usize, &'a str)>>,
    arena: &mut Vec<Node>,
) -> NodeId {
    let (line_index, line) = it.next().unwrap();

    let (without_tags, tags) = tag::extract_tags(line.to_string());
    let without_tags = without_tags.trim();

    let (kind, text): (_, Cow<str>) = match classify(&without_tags) {
        LineKind::Task => (ItemKind::Task, Cow::Owned(parse_task_text(&without_tags))),
        LineKind::Project => (
            ItemKind::Project,
            Cow::Owned(parse_project_text(&without_tags)),
        ),
        LineKind::Note => (ItemKind::Note, Cow::Borrowed(without_tags)),
    };

    let indent = find_indent(line);
    arena.push(Node {
        parent: None,
        children: Vec::new(),
        item: Item {
            indent,
            kind,
            text: text.to_string(),
            tags,
            line_index: Some(line_index),
        },
    });
    let node_id = NodeId(arena.len() - 1);

    let mut children = Vec::new();
    loop {
        match it.peek() {
            Some((_, next_line)) if find_indent(next_line) <= indent => break,
            None => break,
            Some(_) => (),
        }
        let child_node = parse_item(it, arena);
        arena[child_node.0].parent = Some(node_id.clone());
        children.push(child_node);
    }
    arena[node_id.0].children = children;

    node_id
}

#[derive(Debug)]
pub struct TaskpaperFile {
    arena: Vec<Node>,
    nodes: Vec<NodeId>,

    /// If this was loaded from a file, this will be set to the path of that file.
    path: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub enum Position<'a> {
    AsFirst,
    AsLast,
    AsFirstChildOf(&'a NodeId),
    AsLastChildOf(&'a NodeId),
    After(&'a NodeId),
}

impl TaskpaperFile {
    pub fn new() -> Self {
        TaskpaperFile {
            arena: Vec::new(),
            nodes: Vec::new(),
            path: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref().map(|p| p as &Path)
    }

    pub fn parse_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::parse_file_with_content(&::std::fs::read_to_string(&path)?, path)
    }

    pub fn parse_file_with_content(input: &str, path: impl AsRef<Path>) -> Result<Self> {
        let mut s = Self::parse(&input)?;
        s.path = Some(path.as_ref().to_path_buf());
        Ok(s)
    }

    pub fn parse(input: &str) -> Result<Self> {
        // TODO(sirver): Swift does not filter empty line and that feels more correct.
        let mut it = input
            .trim()
            .lines()
            .enumerate()
            .filter(|(_line_index, line)| !line.trim().is_empty())
            .peekable();

        let mut nodes = Vec::new();
        let mut arena = Vec::new();

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
            item,
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

    pub fn insert(&mut self, item: Item, position: Position) -> NodeId {
        let node_id = self.register_item(item);
        self.insert_node(node_id.clone(), position);
        node_id
    }

    pub fn insert_node(&mut self, node_id: NodeId, position: Position) {
        // Ensure that the indentation of the child is at least the parent + 1.
        let ensure_indent_larger_then_parent = |arena: &mut [Node], parent_id: &NodeId| {
            let indent = cmp::max(
                arena[parent_id.0].item().indent + 1,
                arena[node_id.0].item().indent,
            );
            arena[node_id.0].item_mut().indent = indent;
        };

        match position {
            Position::AsFirst => {
                self.arena[node_id.0].parent = None;
                self.nodes.insert(0, node_id);
            }
            // NOCOM(#sirver): does clippy catch this?
            Position::AsLast => {
                self.arena[node_id.0].parent = None;
                self.nodes.push(node_id.clone());
            }
            Position::AsFirstChildOf(parent_id) => {
                ensure_indent_larger_then_parent(&mut self.arena, parent_id);
                self.arena[node_id.0].parent = Some(parent_id.clone());
                self.arena[parent_id.0].children.insert(0, node_id)
            }
            Position::AsLastChildOf(parent_id) => {
                ensure_indent_larger_then_parent(&mut self.arena, parent_id);
                self.arena[node_id.0].parent = Some(parent_id.clone());
                self.arena[parent_id.0].children.push(node_id)
            }
            Position::After(sibling_id) => {
                let parent_id = self.arena[sibling_id.0].parent.clone().expect(
                    "Passing Position::After with a node that has no parent is unexpected.",
                );
                ensure_indent_larger_then_parent(&mut self.arena, &parent_id);
                self.arena[node_id.0].parent = Some(parent_id.clone());
                let parent_node = &mut self.arena[parent_id.0];
                let position = parent_node
                    .children
                    .iter()
                    .position(|id| *id == *sibling_id)
                    .expect("Sibling not actually a child of parent.");
                parent_node.children.insert(position + 1, node_id);
            }
        };
    }

    pub fn to_string(&self, options: FormatOptions) -> String {
        let mut buf = String::new();
        print_nodes(self.nodes.clone(), &self.arena, &mut buf, 0, options)
            .expect("Formatting should never fail.");
        buf
    }

    pub fn node_to_string(&self, node_id: &NodeId) -> String {
        let mut buf = String::new();
        let item = self.arena[node_id.0].item();
        match &item.kind {
            ItemKind::Project => append_project_to_string(item, &mut buf, 0)
                .expect("Writing to string should always work."),
            ItemKind::Task => append_task_to_string(item, &mut buf, 0)
                .expect("Writing to string should always work."),
            ItemKind::Note => append_note_to_string(item, &mut buf, 0)
                .expect("Writing to string should always work."),
        };
        buf
    }

    pub fn write(&self, path: impl AsRef<Path>, options: FormatOptions) -> Result<()> {
        let new = self.to_string(options);

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
            if expr.evaluate(&node.item().tags).is_truish() {
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
                if expr.evaluate(&arena[node_id.0].item.tags).is_truish() {
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

    pub fn iter_node(&self, node_id: &NodeId) -> TaskpaperIter {
        let mut open = VecDeque::new();
        open.push_back(node_id.clone());
        TaskpaperIter { tpf: self, open }
    }

    pub fn iter_node_mut(&mut self, node_id: &NodeId) -> TaskpaperIterMut {
        let mut open = VecDeque::new();
        open.push_back(node_id.clone());
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

// TODO(sirver): IterItem and IterItemMut seem unnecessary, they are essentially Nodes, but they
// protect the nodes from being changed during iteration. Maybe that is worth having another layer
// of abstraction over this.
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

// TODO(sirver): Move this function to taskpaper_cli, since it is fairly specific to my current
// usecases.
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
    let mut pairs = Vec::new();

    for dest_node in destination.iter() {
        if let Some(source_node) = source
            .iter()
            .find(|source_node| source_node.item().text() == dest_node.item().text())
        {
            pairs.push((source_node.id().clone(), dest_node.id().clone()));
        }
    }

    for (source_id, destination_id) in pairs {
        // TODO(sirver): This needs reconsideration.
        // As for its children: we copy over all notes unchanged, but ignore every other
        // children. This is a tad iffy, because we remove and add children to the 'dest_node'
        // while we iterate over it. Current implementation behavior is that these changes are
        // ignored by the iteration (since the children of the current node are pushed to the
        // open list before the node is visited).
        let source_node = &source[&source_id];
        match (
            &source_node.item().kind,
            &destination[&destination_id].item().kind,
        ) {
            (ItemKind::Project, ItemKind::Project) | (ItemKind::Task, ItemKind::Task) => {
                // Copy the data of the changed item over.
                *destination[&destination_id].item_mut() = source_node.item().clone();
            }
            _ => continue,
        };

        if source_node.children().is_empty() {
            continue;
        }

        // Unlink all existing Notes from destination.
        let children_to_nuke = destination[&destination_id]
            .children
            .iter()
            .filter(|id| destination[&id].item().is_note())
            .cloned()
            .collect::<Vec<_>>();
        for child_id in children_to_nuke {
            destination.unlink_node(child_id);
        }

        // Copy all notes from other over.
        for source_child_id in source_node.children() {
            if !source[source_child_id].item().is_note() {
                continue;
            }
            let dest_child_id = destination.copy_node(&source, source_child_id);
            destination.insert_node(dest_child_id, Position::AsLastChildOf(&destination_id));
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
        let golden = vec![Item {
            indent: 0,
            kind: ItemKind::Task,
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
        }];
        let output = TaskpaperFile::parse(&input).unwrap();
        let items: Vec<Item> = output.iter().map(|n| n.item().clone()).collect();
        assert_eq!(golden, items);
    }

    #[test]
    fn test_task_with_mixed_tags_parse() {
        let input = r"- A task @done(2018-08-05) @another(foo bar) @tag1 @tag2";
        let golden = vec![Item {
            indent: 0,
            kind: ItemKind::Task,
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
        }];
        let output = TaskpaperFile::parse(&input).unwrap();
        let items: Vec<Item> = output.iter().map(|n| n.item().clone()).collect();
        assert_eq!(golden, items);
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
        let golden = "- Arbeit • Foo • blah @coding @next @blocked(arg prs) @done(2018-06-21)\n";
        assert_eq!(golden, tpf.to_string(FormatOptions::default()));
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
            &destination.to_string(FormatOptions::default()),
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
            &destination.to_string(FormatOptions::default()),
            include_str!("tests/mirror_changes/destination_golden.taskpaper"),
        );
    }
}
