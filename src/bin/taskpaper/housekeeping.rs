use crate::ConfigurationFile;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use structopt::StructOpt;
use taskpaper::{Result, TaskpaperFile};

#[derive(Debug, Serialize, Deserialize)]
struct Formats {
    formats: HashMap<String, taskpaper::FormatOptions>,
}

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

pub fn run(_: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let mut inbox = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Inbox)?;
    let mut todo = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Todo)?;
    let mut tickle = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Tickle)?;

    crate::tickle::tickle(&mut inbox, &mut todo, &mut tickle)?;
    crate::extract_checkout::extract_checkout(&mut todo)?;
    crate::extract_timeline::extract_timeline(&mut todo, config)?;
    // It is very important to first write todo.taskpaper, so that the extract methods that might
    // be run now on file change do not run into an infinite loop.
    todo.overwrite_common_file(taskpaper::CommonFileKind::Todo, config.formats["todo"])?;
    inbox.overwrite_common_file(taskpaper::CommonFileKind::Inbox, config.formats["inbox"])?;
    tickle.overwrite_common_file(taskpaper::CommonFileKind::Tickle, config.formats["inbox"])?;

    Ok(())
}
