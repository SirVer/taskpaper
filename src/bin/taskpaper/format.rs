use crate::ConfigurationFile;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{Error, Result, TaskpaperFile};

#[derive(Debug, Serialize, Deserialize)]
struct Formats {
    formats: HashMap<String, taskpaper::FormatOptions>,
}

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to read.
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Style to format with. The default is 'default' for free standing files and as configured
    /// for files within the Database.
    #[structopt(short = "-s", long = "--style")]
    style: Option<String>,
}

pub fn format(args: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    // NOCOM(#sirver): If it is inside the database, let the database handle writing (and config)
    // let db = taskpaper::db::Database::read(&config.database)?;
    // let input = args.input.absolutize()?;
    // if let Some(rel) = db.relative(&args.input) {
    // if config.formats.contains_key(rel.to_string_lossy().as_ref()) {
    // println!("Formatting
    // }
    // }
    let style = match args.style.as_ref() {
        None => taskpaper::FormatOptions::default(),
        Some(s) => match config.formats.get(s) {
            Some(format) => *format,
            None => return Err(Error::misc(format!("Style '{}' not found.", s))),
        },
    };

    let taskpaper_file = TaskpaperFile::parse_file(&args.input)?;
    taskpaper_file.write(&args.input, style)?;
    Ok(())
}
