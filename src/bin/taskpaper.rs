use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{TaskpaperFile, ToStringWithIndent};

/// Command-line client to interact with taskpaper files.
#[derive(StructOpt, Debug)]
#[structopt(name = "taskpaper")]
struct CommandLineArguments {
    /// File to read.
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Search query to run against the file.
    #[structopt(short = "-s")]
    search: Option<String>,

    /// Print descendants (notes & children) for results.
    #[structopt(short = "-d")]
    descendants: bool,
}

fn main() {
    let args = CommandLineArguments::from_args();

    let taskpaper_file = TaskpaperFile::parse_file(args.input).unwrap();

    if let Some(search) = args.search {
        let results = taskpaper_file.search(&search).unwrap();
        print!(
            "{}",
            results.to_string(taskpaper::FormatOptions {
                print_children: if args.descendants {
                    taskpaper::PrintChildren::Yes
                } else {
                    taskpaper::PrintChildren::No
                },
                print_notes: if args.descendants {
                    taskpaper::PrintNotes::Yes
                } else {
                    taskpaper::PrintNotes::No
                },
                ..Default::default()
            })
        );
    } else {
        print!(
            "{}",
            taskpaper_file.to_string(taskpaper::FormatOptions::default())
        );
    }
}
