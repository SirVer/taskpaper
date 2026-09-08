use anyhow::{Result, anyhow};
use clap::Args;
use std::path::PathBuf;
use taskpaper::{Database, TaskpaperFile};

#[derive(Args, Debug)]
pub struct CommandLineArguments {
    /// File to read.
    input: PathBuf,

    /// Style to format with. The default is 'default'.
    #[arg(short = 's', long = "style")]
    style: Option<String>,
}

pub fn format(db: &Database, args: &CommandLineArguments) -> Result<()> {
    let config = db.config()?;
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
