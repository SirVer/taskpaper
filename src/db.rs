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
    pub files: HashMap<PathBuf, TaskpaperFile>,
}

impl Database {
    pub fn read(dir: impl AsRef<Path>) -> Result<Self> {
        let mut files = HashMap::new();
        let root = dir.as_ref().absolutize()?;
        for entry in WalkDir::new(dir.as_ref()) {
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
            let relative_path = entry
                .path()
                .strip_prefix(dir.as_ref())
                .unwrap()
                .to_path_buf();
            files.insert(relative_path, file.unwrap());
        }
        Ok(Database { root, files })
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
        if rel != canon {
            Some(rel.to_path_buf())
        } else {
            None
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
            "- before entry @tickle(2018-09-01)\n \
             - after entry @tickle(2018-10-02)\n",
        );

        let _db = t.read_database();

        // TODO(sirver): This test does nothing currently.
    }
}
