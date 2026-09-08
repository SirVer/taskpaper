use anyhow::{Result, anyhow};
use clap::Args;
use std::path::PathBuf;
use taskpaper::{Database, TaskpaperFile};

#[derive(Args, Debug)]
pub struct CommandLineArguments {
    /// File to modify.
    #[arg(required = true)]
    input: PathBuf,

    /// Tags to purge (including the @).
    tags: Vec<String>,

    /// Style to format with. The default is 'default'.
    #[arg(short = 's', long = "style", default_value = "default")]
    style: String,
}

pub fn run(db: &Database, args: &CommandLineArguments) -> Result<()> {
    let config = db.config()?;
    let style = match config.formats.get(&args.style) {
        Some(format) => *format,
        None => return Err(anyhow!("Style '{}' not found.", args.style)),
    };

    let mut input = TaskpaperFile::parse_file(&args.input)?;
    for mut node in &mut input {
        for t in &args.tags {
            node.item_mut().tags_mut().remove(t.trim_start_matches('@'));
        }
    }

    input.write(&args.input, style)?;
    Ok(())
}
