use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::TaskpaperFile;

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to modify.
    #[structopt(parse(from_os_str), long = "--input", short = "-i")]
    input: PathBuf,

    /// Query of the items to delete.
    query: String,
}

pub fn run(args: &CommandLineArguments) -> Result<()> {
    let mut input = TaskpaperFile::parse_file(&args.input)?;
    input.filter(&args.query)?;
    input.write(&args.input)?;
    Ok(())
}
