use crate::{Config, FormatOptions};
use crate::{Result, TaskpaperFile};
use path_absolutize::Absolutize;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

    fn get_format_for_filename(&self, path: &Path) -> Result<FormatOptions> {
        let stem = path
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
