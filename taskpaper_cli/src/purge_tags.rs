use crate::ConfigurationFile;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{Error, Item, Result, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to modify.
    #[structopt(parse(from_os_str), required = true)]
    input: PathBuf,

    /// Tags to purge (including the @).
    tags: Vec<String>,

    /// Style to format with. The default is 'default'.
    #[structopt(short = "-s", long = "--style", default_value = "default")]
    style: String,
}

pub fn run(args: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let style = match config.formats.get(&args.style) {
        Some(format) => *format,
        None => return Err(Error::misc(format!("Style '{}' not found.", args.style))),
    };

    let mut input = TaskpaperFile::parse_file(&args.input)?;
    input.map_mut(|item| {
        let tags = match item {
            Item::Project(ref mut p) => &mut p.tags,
            Item::Task(ref mut t) => &mut t.tags,
        };
        for t in &args.tags {
            tags.remove(t.trim_start_matches("@"));
        }
    });

    input.write(&args.input, style)?;
    Ok(())
}
