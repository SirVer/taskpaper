use crate::ConfigurationFile;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use structopt::StructOpt;
use taskpaper::{Database, Result};

#[derive(Debug, Serialize, Deserialize)]
struct Formats {
    formats: HashMap<String, taskpaper::FormatOptions>,
}

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

pub fn run(db: &Database, _: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let mut inbox = db.parse_common_file(taskpaper::CommonFileKind::Inbox)?;
    let mut todo = db.parse_common_file(taskpaper::CommonFileKind::Todo)?;
    let mut tickle = db.parse_common_file(taskpaper::CommonFileKind::Tickle)?;

    crate::tickle::tickle(&mut inbox, &mut todo, &mut tickle)?;
    crate::extract_checkout::extract_checkout(db, &mut todo)?;
    crate::extract_timeline::extract_timeline(db, &mut todo, config)?;
    // It is very important to first write todo.taskpaper, so that the extract methods that might
    // be run now on file change do not run into an infinite loop.
    db.overwrite_common_file(
        &todo,
        taskpaper::CommonFileKind::Todo,
        config.formats["todo"],
    )?;
    db.overwrite_common_file(
        &inbox,
        taskpaper::CommonFileKind::Inbox,
        config.formats["inbox"],
    )?;
    db.overwrite_common_file(
        &tickle,
        taskpaper::CommonFileKind::Tickle,
        config.formats["inbox"],
    )?;

    Ok(())
}
