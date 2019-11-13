use self_update::cargo_crate_version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use structopt::StructOpt;
use taskpaper;

mod check_feeds;
#[cfg(target_os = "macos")]
mod dump_reading_list;
mod extract_checkout;
mod extract_timeline;
mod filter;
mod format;
mod housekeeping;
mod log_done;
mod merge_timelines;
mod purge_tags;
mod search;
mod tickle;
mod to_inbox;

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigurationFile {
    database: String,
    formats: HashMap<String, taskpaper::FormatOptions>,
    aliases: HashMap<String, String>,
    feeds: Vec<check_feeds::FeedConfiguration>,
}

fn update() -> Result<(), Box<dyn ::std::error::Error>> {
    let target = self_update::get_target();
    self_update::backends::github::Update::configure()
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
#[structopt(rename_all = "verbatim")]
enum Command {
    /// Add items to the inbox.
    /// This is smart about ',' and '.' as first entries to add a note with the contents of the
    /// clipboard to every task that is added. Under Linux ',' is primary, i.e. the last mouse
    /// selection, while '.' is the X11 clipboard (copy & pasted). There is no distinction under Mac OS
    /// since there is only one clipboard.
    #[structopt(name = "2inbox")]
    ToInbox(to_inbox::CommandLineArguments),

    /// Format a taskpaper file, without introducing any other changes.
    #[structopt(name = "format")]
    Format(format::CommandLineArguments),

    /// Housekeeping after any file has changed. This includes extracting the timeline and the
    /// checkout, as well as formatting todo and inbox.
    #[structopt(name = "housekeeping")]
    Housekeeping(housekeeping::CommandLineArguments),

    #[structopt(name = "search")]
    Search(search::CommandLineArguments),

    /// Merge one taskpaper file into another on a project level. All top level items that are not
    /// in both are appended to the end.
    #[structopt(name = "merge_timelines")]
    MergeTimelines(merge_timelines::CommandLineArguments),

    /// Log everything marked as done into the logbook.
    #[structopt(name = "log_done")]
    LogDone(log_done::CommandLineArguments),

    /// Dump reading list. Dumps the reading list as items ready to go into the Inbox.
    #[cfg(target_os = "macos")]
    #[structopt(name = "dump_reading_list")]
    DumpReadingList(dump_reading_list::CommandLineArguments),

    /// Remove all of the given tags in the given file.
    #[structopt(name = "purge_tags")]
    PurgeTags(purge_tags::CommandLineArguments),

    /// Remove all items matching the query from the input
    #[structopt(name = "filter_out")]
    Filter(filter::CommandLineArguments),

    /// Checks all configured RSS feeds and puts them into the Inbox.
    #[structopt(name = "check_feeds")]
    CheckFeeds(check_feeds::CommandLineArguments),
}

fn main() {
    let args = CommandLineArguments::from_args();
    if args.update {
        update().unwrap();
        return;
    }

    let home = dirs::home_dir().expect("HOME not set.");
    let config: ConfigurationFile = {
        let data = std::fs::read_to_string(home.join(".taskpaperrc"))
            .expect("Could not read ~/.taskpaperrc.");
        let mut config: ConfigurationFile =
            toml::from_str(&data).expect("Could not parse ~/.taskpaperrc.");
        config.database =
            shellexpand::tilde_with_context(&config.database, dirs::home_dir).to_string();
        config
    };

    let db = taskpaper::Database::from_dir(&config.database).expect("Could not open the database.");

    match args.cmd {
        Some(Command::Search(args)) => search::search(&db, &args, &config).unwrap(),
        Some(Command::ToInbox(args)) => to_inbox::to_inbox(&db, &args, &config).unwrap(),
        Some(Command::Format(args)) => format::format(&args, &config).unwrap(),
        Some(Command::Housekeeping(args)) => housekeeping::run(&db, &args, &config).unwrap(),
        Some(Command::MergeTimelines(args)) => merge_timelines::run(&args, &config).unwrap(),
        Some(Command::LogDone(args)) => log_done::run(&db, &args, &config).unwrap(),
        Some(Command::PurgeTags(args)) => purge_tags::run(&args, &config).unwrap(),
        Some(Command::Filter(args)) => filter::run(&args, &config).unwrap(),
        Some(Command::CheckFeeds(args)) => check_feeds::run(&db, &args, &config).unwrap(),

        #[cfg(target_os = "macos")]
        Some(Command::DumpReadingList(args)) => dump_reading_list::dump_reading_list(&db, &args),
        None => {
            // TODO(sirver): I found no easy way to make clap output the usage here.
            println!("Need a subcommand.");
            std::process::exit(1);
        }
    }
}
