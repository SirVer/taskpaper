use crate::ConfigurationFile;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::TaskpaperFile;

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
    let style = match args.style.as_ref() {
        None => taskpaper::FormatOptions::default(),
        Some(s) => match config.formats.get(s) {
            Some(format) => *format,
            None => return Err(anyhow!("Style '{}' not found.", s)),
        },
    };

    let mut taskpaper_file = TaskpaperFile::parse_file(&args.input)?;
    taskpaper_file.format(style);
    taskpaper_file.write(&args.input)?;
    Ok(())
}
