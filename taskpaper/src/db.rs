use crate::{Config, FormatOptions};
use crate::{Result, TaskpaperFile};
use path_absolutize::Absolutize;
use std::cmp;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
enum SortDir {
    Desc,
    Asc,
}

#[derive(Debug)]
struct SortBy {
    key: String,
    dir: SortDir,
}

fn get_sort_values(
    tpf: &TaskpaperFile,
    sorting_set: &[SortBy],
    node_id: &crate::NodeId,
    path: &Path,
    line_no: usize,
) -> Vec<Option<String>> {
    let mut values = Vec::new();
    let tags = tpf[node_id].item().tags();
    for s in sorting_set {
        match tags.get(&s.key) {
            None => values.push(None),
            Some(t) => match &t.value {
                None => values.push(None),
                Some(v) => values.push(Some(v.to_string())),
            },
        }
    }
    // As tiebreaker, we use (path, string)
    values.push(Some(path.to_string_lossy().to_string()));
    values.push(Some(format!("{:05}", line_no)));
    values
}

pub struct Match<'a> {
    pub tpf: &'a TaskpaperFile,
    pub path: &'a Path,
    pub line_no: usize,
    pub node_id: crate::NodeId,
}

// TODO(hrapp): This seems messy - on the one site, this should be part of the Database, on the
// other site this is used in the App too. It is also questionable if all files should be searched
// or only one.
pub fn search<'a>(
    mut query: String,
    sort_by: Option<&str>,
    config: &Config,
    files_map: &'a HashMap<PathBuf, impl AsRef<TaskpaperFile>>,
) -> Result<Vec<Match<'a>>> {
    'outer: for _ in 0..50 {
        for (key, value) in &config.aliases {
            let new_query = query.replace(key, value);
            if new_query != query {
                query = new_query;
                continue 'outer;
            }
        }
    }

    let sort_order = sort_by.as_ref().map(|s| {
        let mut res = Vec::new();
        for entry in s.split(",") {
            let entry = entry.trim();
            if entry.starts_with("-") {
                res.push(SortBy {
                    key: entry.trim_start_matches("-").to_string(),
                    dir: SortDir::Desc,
                })
            } else {
                res.push(SortBy {
                    key: entry.to_string(),
                    dir: SortDir::Asc,
                })
            }
        }
        res
    });

    let mut files = Vec::new();
    for (path, tpf) in files_map {
        if let Some(name) = path.file_name() {
            if config
                .search
                .excluded_files
                .contains(name.to_string_lossy().as_ref())
            {
                continue;
            }
        }
        files.push((path, tpf));
    }

    let mut searches: HashMap<&Path, _> = HashMap::new();
    for (path, tpf) in files {
        searches.insert(path as &Path, (tpf.as_ref().search(&query)?, tpf.as_ref()));
    }

    let mut matches = Vec::new();
    for path in searches.keys() {
        let (node_ids, tpf) = &searches[&path as &Path];
        if node_ids.is_empty() {
            continue;
        }

        for node_id in node_ids.iter() {
            let item = tpf[node_id].item();
            matches.push(Match {
                tpf,
                path,
                line_no: item.line_index().unwrap() + 1,
                node_id: node_id.clone(),
            });
        }
    }

    if let Some(ref s) = sort_order {
        matches.sort_by(|a, b| {
            let val_a = get_sort_values(a.tpf, &s, &a.node_id, a.path, a.line_no);
            let val_b = get_sort_values(b.tpf, &s, &b.node_id, b.path, b.line_no);
            for (idx, s) in s.iter().enumerate() {
                let res = match s.dir {
                    SortDir::Asc => val_a[idx].cmp(&val_b[idx]),
                    SortDir::Desc => val_b[idx].cmp(&val_a[idx]),
                };
                match res {
                    cmp::Ordering::Less | cmp::Ordering::Greater => return res,
                    cmp::Ordering::Equal => (),
                }
            }
            cmp::Ordering::Equal
        });
    }

    Ok(matches)
}

/// A folder containing many Taskpaper files. Some of which are special, like inbox, timeline.
#[derive(Debug)]
pub struct Database {
    pub root: PathBuf,
}

impl Database {
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let root = dir.as_ref().absolutize()?.to_path_buf();
        Ok(Self { root })
    }

    pub fn config(&self) -> Result<Config> {
        let data = std::fs::read_to_string(self.root.join(".config.toml"))?;
        Ok(toml::from_str(&data).map_err(|e| crate::Error::InvalidConfig(e.to_string()))?)
    }

    pub fn parse_all_files(&self) -> Result<HashMap<PathBuf, TaskpaperFile>> {
        let mut files = HashMap::new();
        for entry in WalkDir::new(&self.root) {
            if entry.is_err() {
                continue;
            }
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension() != Some(OsStr::new("taskpaper")) {
                continue;
            }
            let file = TaskpaperFile::parse_file(path);
            if file.is_err() {
                println!("Skipping {:?} due to parsing errors.", path);
                continue;
            }
            let relative_path = entry.path().strip_prefix(&self.root).unwrap().to_path_buf();
            files.insert(relative_path, file.unwrap());
        }
        Ok(files)
    }

    /// Returns the name (i.e. relative path) of 'path' inside of the database.
    pub fn relative(&self, path: impl AsRef<Path>) -> Option<PathBuf> {
        let canon = match path.as_ref().absolutize() {
            Err(_) => return None,
            Ok(a) => a,
        };
        let rel = match canon.strip_prefix(&self.root) {
            Err(_) => return None,
            Ok(a) => a,
        };
        if rel == canon {
            None
        } else {
            Some(rel.to_path_buf())
        }
    }

    pub fn parse_common_file(&self, kind: CommonFileKind) -> Result<TaskpaperFile> {
        TaskpaperFile::parse_file(kind.find(&self.root).expect("Common file not found!"))
    }

    pub fn get_format_for_filename(&self, path: impl AsRef<Path>) -> Result<FormatOptions> {
        let stem = path
            .as_ref()
            .file_stem()
            .expect("Always a filestem")
            .to_string_lossy();
        let config = self.config()?;
        for name in [stem.as_ref(), "default"] {
            if let Some(f) = config.formats.get(name) {
                return Ok(f.clone());
            }
        }
        Ok(FormatOptions::default())
    }

    pub fn overwrite_common_file(&self, tpf: &TaskpaperFile, kind: CommonFileKind) -> Result<()> {
        let format = self.get_format_for_filename(&kind.to_path_buf())?;
        tpf.write(
            kind.find(&self.root).expect("Common file not found!"),
            format,
        )
    }

    pub fn path_of_common_file(&self, kind: CommonFileKind) -> Option<PathBuf> {
        kind.find(&self.root)
    }
}

#[derive(Debug)]
pub enum CommonFileKind {
    Inbox,
    Todo,
    Tickle,
    Logbook,
    Timeline,
}

impl CommonFileKind {
    fn find(&self, root: &Path) -> Option<PathBuf> {
        let path = root.join(self.to_path_buf());
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    fn to_path_buf(&self) -> PathBuf {
        match *self {
            CommonFileKind::Inbox => PathBuf::from("01_inbox.taskpaper"),
            CommonFileKind::Todo => PathBuf::from("02_todo.taskpaper"),
            CommonFileKind::Tickle => PathBuf::from("03_tickle.taskpaper"),
            CommonFileKind::Logbook => PathBuf::from("40_logbook.taskpaper"),
            CommonFileKind::Timeline => PathBuf::from("10_timeline.taskpaper"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::DatabaseTest;
    use crate::CommonFileKind;

    // TODO(sirver): Actually add a few tests for tickling, timeline and so on?
    #[test]
    fn test_tickle_file() {
        let mut t = DatabaseTest::new();
        t.write_file(
            CommonFileKind::Inbox.to_path_buf(),
            "- to tickle @tickle(2018-10-01)\n",
        );
        t.write_file(
            CommonFileKind::Tickle.to_path_buf(),
            "- before item @tickle(2018-09-01)\n \
             - after item @tickle(2018-10-02)\n",
        );

        let _db = t.read_database();

        // TODO(sirver): This test does nothing currently.
    }
}
