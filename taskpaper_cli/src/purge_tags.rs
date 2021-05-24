use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::TaskpaperFile;

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to modify.
    #[structopt(parse(from_os_str), required = true)]
    input: PathBuf,

    /// Tags to purge (including the @).
    tags: Vec<String>,
}

pub fn run(args: &CommandLineArguments) -> Result<()> {
    let mut input = TaskpaperFile::parse_file(&args.input)?;
    for mut node in &mut input {
        for t in &args.tags {
            node.item_mut().tags_mut().remove(t.trim_start_matches('@'));
        }
    }
    input.write(&args.input)?;
    Ok(())
}
