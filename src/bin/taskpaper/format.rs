use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{Result, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to read.
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

pub fn format(args: &CommandLineArguments) -> Result<()> {
    let taskpaper_file = TaskpaperFile::parse_file(&args.input)?;
    taskpaper_file.write(
        &args.input,
        taskpaper::FormatOptions {
            sort: taskpaper::Sort::Nothing,
            empty_line_after_project: taskpaper::EmptyLineAfterProject {
                top_level: 2,
                first_level: 0,
                others: 0,
            },
            ..Default::default()
        },
    )?;
    Ok(())
}
