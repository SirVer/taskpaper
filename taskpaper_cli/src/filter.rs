use anyhow::{Result, anyhow};
use clap::Args;
use std::path::PathBuf;
use taskpaper::{Database, TaskpaperFile};

#[derive(Args, Debug)]
pub struct CommandLineArguments {
    /// File to modify.
    #[arg(long = "input", short = 'i')]
    input: PathBuf,

    /// Style to format with. The default is 'default'.
    #[arg(short = 's', long = "style")]
    style: String,

    /// Query of the items to delete.
    query: String,
}

pub fn run(db: &Database, args: &CommandLineArguments) -> Result<()> {
    let config = db.config()?;
    let style = match config.formats.get(&args.style) {
        Some(format) => *format,
        None => return Err(anyhow!("Style '{}' not found.", args.style)),
    };

    let mut input = TaskpaperFile::parse_file(&args.input)?;
    input.filter(&args.query)?;
    input.write(&args.input, style)?;
    Ok(())
}
