use std::fs::{self};
use std::path::{Path, PathBuf};
use tempdir::TempDir;

use crate::db::Database;

/// Sets up a directory in which files can be dumped. This directory can be loaded as database,
/// modified and then asserted over.
#[derive(Debug)]
pub struct DatabaseTest {
    dir: TempDir,
    database: Option<Database>,
}

impl DatabaseTest {
    pub fn new() -> Self {
        let dir = TempDir::new("taskpaper_db_test").expect("Could not create tempdir.");
        DatabaseTest {
            dir,
            database: None,
        }
    }

    pub fn write_file(&self, path: impl AsRef<Path>, content: &str) -> PathBuf {
        let file_path = self.dir.path().join(path);
        fs::write(&file_path, content.as_bytes()).expect("Could not write file");
        file_path
    }

    pub fn read_file(&self, path: impl AsRef<Path>) -> String {
        let file_path = self.dir.path().join(path);
        fs::read_to_string(&file_path).expect("Could not read file")
    }

    pub fn read_database(&mut self) -> &mut Database {
        let db = Database::from_dir(self.dir.path()).expect("Could not read database.");
        self.database = Some(db);
        self.database.as_mut().unwrap()
    }

    pub fn assert_eq_to_golden(&self, golden: impl AsRef<Path>, path: impl AsRef<Path>) {
        let golden_data = fs::read_to_string(golden.as_ref()).expect("Could not read golden.");
        let out = fs::read_to_string(&self.dir.path().join(path.as_ref()))
            .expect("Could not read golden.");
        if golden_data == out {
            return;
        }

        let tmp_path = PathBuf::from("/tmp").join(&path);
        ::std::fs::write(&tmp_path, &out).expect("Could not write output.");

        panic!(
            "{} != {}.\n\nWrote output into {}",
            golden.as_ref().display(),
            path.as_ref().display(),
            tmp_path.display()
        );
    }
}
