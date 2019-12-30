use crate::ConfigurationFile;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{Error, Result, TaskpaperFile};

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

pub fn run(args: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let style = match config.formats.get(&args.style) {
        Some(format) => *format,
        None => return Err(Error::misc(format!("Style '{}' not found.", args.style))),
    };

    let mut input = TaskpaperFile::parse_file(&args.input)?;
    input.filter(&args.query)?;
    input.write(&args.input, style)?;
    Ok(())
}
