use self_update::cargo_crate_version;
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{TaskpaperFile, ToStringWithIndent};

#[cfg(target_os = "macos")]
mod dump_reading_list;
mod to_inbox;

fn update() -> Result<(), Box<::std::error::Error>> {
    let target = self_update::get_target()?;
    self_update::backends::github::Update::configure()?
        .repo_owner("SirVer")
        .repo_name("taskpaper")
        .target(&target)
        .bin_name("taskpaper")
        .show_download_progress(true)
        .show_output(false)
        .no_confirm(true)
        .current_version(cargo_crate_version!())
        .build()?
        .update()?;
    Ok(())
}

/// Command-line client to interact with taskpaper files.
#[derive(StructOpt, Debug)]
#[structopt(name = "taskpaper")]
struct CommandLineArguments {
    /// Update binary in-place from latest release.
    #[structopt(long = "--update")]
    update: bool,

    #[structopt(subcommand)]
    cmd: Option<Command>,
}

#[derive(StructOpt, Debug)]
struct SearchArgs {
    /// File to read.
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Search query to run against the file.
    search: String,

    /// Print descendants (notes & children) for results.
    #[structopt(short = "-d")]
    descendants: bool,
}

#[derive(StructOpt, Debug)]
enum Command {
    /// Write items into the Inbox.
    #[structopt(name = "2inbox")]
    ToInbox(to_inbox::CommandLineArguments),

    #[structopt(name = "search")]
    Search(SearchArgs),

    /// Dump reading list. Dumps the reading list as items ready to go into the Inbox.
    #[cfg(target_os = "macos")]
    #[structopt(name = "dump_reading_list")]
    DumpReadingList(dump_reading_list::CommandLineArguments),
}

fn main() {
    let args = CommandLineArguments::from_args();
    if args.update {
        update().unwrap();
        return;
    }

    match args.cmd {
        Some(Command::Search(args)) => {
            let taskpaper_file = TaskpaperFile::parse_file(args.input).unwrap();
            let results = taskpaper_file.search(&args.search).unwrap();
            print!(
                "{}",
                results.to_string(taskpaper::FormatOptions {
                    sort: taskpaper::Sort::Nothing,
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
        }
        Some(Command::ToInbox(args)) => to_inbox::to_inbox(&args),

        #[cfg(target_os = "macos")]
        Some(Command::DumpReadingList(args)) => dump_reading_list::dump_reading_list(&args),
        None => {
            // TODO(sirver): I found no easy way to make clap output the usage here.
            println!("Need a subcommand.");
            std::process::exit(1);
        }
    }
}
