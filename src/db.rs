use crate::{Result, TaskpaperFile};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A folder containing many Taskpaper files. Some of which are special, like inbox, timeline.
#[derive(Debug)]
pub struct Database {
    files: HashMap<PathBuf, TaskpaperFile>,
}

impl Database {
    pub fn read(dir: impl AsRef<Path>) -> Result<Self> {
        let mut files = HashMap::new();
        for entry in WalkDir::new(dir.as_ref()) {
            if entry.is_err() {
                continue;
            }
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension() != Some(OsStr::new("taskpaper")) {
                continue;
            }
            let file = TaskpaperFile::parse_file(path)?;
            let relative_path = entry
                .path()
                .strip_prefix(dir.as_ref())
                .unwrap()
                .to_path_buf();
            files.insert(relative_path, file);
        }
        Ok(Database { files })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommonFileKind;
    use std::fs::File;
    use std::io::Write;
    use tempdir::TempDir;

    /// Sets up a directory in which files can be dumped. This directory can be loaded as database,
    /// modified and then asserted over.
    #[derive(Debug)]
    struct DatabaseTest {
        dir: TempDir,
        database: Option<Database>,
    }

    impl DatabaseTest {
        fn new() -> Self {
            let dir = TempDir::new("taskpaper_db_test").expect("Could not create tempdir.");
            DatabaseTest {
                dir,
                database: None,
            }
        }

        fn write_file(&mut self, path: impl AsRef<Path>, content: String) {
            let file_path = self.dir.path().join(path);
            let mut f = File::create(file_path).expect("Could not create file");
            f.write_all(content.as_bytes())
                .expect("Could not write file");
            f.sync_all().unwrap();
        }

        pub fn read_database(&mut self) -> &mut Database {
            self.database =
                Some(Database::read(self.dir.path()).expect("Could not read database."));
            self.database.as_mut().unwrap()
        }
    }

    #[test]
    fn test_tickle_file() {
        let mut t = DatabaseTest::new();
        t.write_file(
            CommonFileKind::Inbox.to_path_buf(),
            "- to tickle @tickle(2018-10-01)\n".to_string(),
        );
        t.write_file(
            CommonFileKind::Tickle.to_path_buf(),
            "- before entry @tickle(2018-09-01)\n \
             - after entry @tickle(2018-10-02)\n"
                .to_string(),
        );

        let _db = t.read_database();

        // NOCOM(#sirver): And here I realized that I require implementing filtering first.
    }
}
