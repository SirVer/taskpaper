use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{Database, TaskpaperFile};

#[derive(Debug, Serialize, Deserialize)]
struct Formats {
    formats: HashMap<String, taskpaper::FormatOptions>,
}

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to read.
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Style to format with. The default is 'default'.
    #[structopt(short = "-s", long = "--style")]
    style: Option<String>,
}

pub fn format(db: &Database, args: &CommandLineArguments) -> Result<()> {
    let config = db.configuration()?;
    let style = match args.style.as_ref() {
        None => taskpaper::FormatOptions::default(),
        Some(s) => match config.formats.get(s) {
            Some(format) => *format,
            None => return Err(anyhow!("Style '{}' not found.", s)),
        },
    };

    let taskpaper_file = TaskpaperFile::parse_file(&args.input)?;
    taskpaper_file.write(&args.input, style)?;
    Ok(())
}
