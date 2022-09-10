use anyhow::{anyhow, Result};
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{Database, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to modify.
    #[structopt(parse(from_os_str), long = "--input", short = "-i")]
    input: PathBuf,

    /// Style to format with. The default is 'default'.
    #[structopt(short = "-s", long = "--style")]
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
