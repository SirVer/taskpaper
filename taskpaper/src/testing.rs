use std::fs::{self};
use std::path::{Path, PathBuf};
use tempdir::TempDir;

use crate::db::Database;

pub fn assert_str_eq_to_golden(id: &str, golden: impl AsRef<Path>, out: &str) {
    let golden_data = fs::read_to_string(golden.as_ref()).expect("Could not read golden.");
    if golden_data == out {
        return;
    }

    let tmp_path = PathBuf::from("/tmp").join(&format!("{}.taskpaper", id));
    ::std::fs::write(&tmp_path, &out).expect("Could not write output.");

    panic!(
        "{} != output.\n\nWrote output into {}",
        golden.as_ref().display(),
        tmp_path.display()
    );
}

pub fn assert_eq_to_golden(golden: impl AsRef<Path>, path: impl AsRef<Path>) {
    let out = fs::read_to_string(path.as_ref()).expect("Could not read out.");
    assert_str_eq_to_golden(
        &path.as_ref().file_name().unwrap().to_string_lossy(),
        golden,
        &out,
    );
}

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

    pub fn assert_eq_to_golden(&self, golden: impl AsRef<Path>, rel_path: impl AsRef<Path>) {
        let full_path = self.dir.path().join(rel_path.as_ref());
        assert_eq_to_golden(golden, &full_path);
    }
}
