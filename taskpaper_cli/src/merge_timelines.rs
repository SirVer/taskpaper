use crate::ConfigurationFile;
use chrono::NaiveDate;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{Error, Item, Result, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to read.
    #[structopt(parse(from_os_str), long = "--from", required = true)]
    from: PathBuf,

    /// File to merge into.
    #[structopt(parse(from_os_str), long = "--into", required = true)]
    into: PathBuf,

    /// Style to format with. The default is 'logbook'.
    #[structopt(short = "-s", long = "--style", default_value = "logbook")]
    style: String,
}

pub fn run(args: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let style = match config.formats.get(&args.style) {
        Some(s) => s,
        None => return Err(Error::misc(format!("Style '{}' not found.", args.style))),
    };

    let from = TaskpaperFile::parse_file(&args.from)?;
    let mut into = TaskpaperFile::parse_file(&args.into)?;

    for e in from.items {
        match e {
            Item::Project(p) => match into.get_project_mut(&p.text) {
                Some(other) => {
                    for e in p.children {
                        other.children.push(e);
                    }
                }
                None => into.items.push(Item::Project(p)),
            },
            Item::Task(_) => into.items.push(e),
        }
    }

    into.items.sort_by_key(|e| match e {
        Item::Project(p) => match NaiveDate::parse_from_str(&p.text, "%A, %d. %B %Y") {
            Ok(v) => v,
            Err(_) => panic!("Encountered unexpected date formatting: {}", p.text),
        },
        _ => panic!("Only expected projects!"),
    });
    into.items.reverse();
    into.write(&args.into, *style)?;
    Ok(())
}
