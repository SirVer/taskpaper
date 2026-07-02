use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

mod check_feeds;
mod extract_timeline;
mod filter;
mod format;
mod housekeeping;
mod log_done;
mod purge_tags;
mod search;
mod tickle;
mod to_inbox;

#[derive(Debug, Serialize, Deserialize)]
pub struct CliConfig {
    database: String,
    feeds: Vec<check_feeds::FeedConfiguration>,
}

/// Command-line client to interact with taskpaper files.
#[derive(Parser, Debug)]
#[command(name = "taskpaper")]
struct CommandLineArguments {
    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "verbatim")]
enum Command {
    /// Add items to the inbox.
    /// This is smart about ',' and '.' as first character to add a note with the contents of the
    /// clipboard to every task that is added. Under Linux ',' is primary, i.e. the last mouse
    /// selection, while '.' is the X11 clipboard (copy & pasted). There is no distinction under
    /// Mac OS since there is only one clipboard.
    #[command(name = "2inbox")]
    ToInbox(to_inbox::CommandLineArguments),

    /// Format a taskpaper file, without introducing any other changes.
    #[command(name = "format")]
    Format(format::CommandLineArguments),

    /// Housekeeping after any file has changed. This includes extracting the timeline and the
    /// checkout, as well as formatting todo and inbox.
    #[command(name = "housekeeping")]
    Housekeeping(housekeeping::CommandLineArguments),

    #[command(name = "search")]
    Search(search::CommandLineArguments),

    /// Log everything marked as done into the logbook.
    #[command(name = "log_done")]
    LogDone(log_done::CommandLineArguments),

    /// Remove all of the given tags in the given file.
    #[command(name = "purge_tags")]
    PurgeTags(purge_tags::CommandLineArguments),

    /// Remove all items matching the query from the input
    #[command(name = "filter_out")]
    Filter(filter::CommandLineArguments),

    /// Checks all configured RSS feeds and puts them into the Inbox.
    #[command(name = "check_feeds")]
    CheckFeeds(check_feeds::CommandLineArguments),
}

fn main() {
    let args = CommandLineArguments::parse();

    let home = dirs::home_dir().expect("HOME not set.");
    let config: CliConfig = {
        let data = std::fs::read_to_string(home.join(".taskpaperrc"))
            .expect("Could not read ~/.taskpaperrc.");
        let mut config: CliConfig = toml::from_str(&data).expect("Could not parse ~/.taskpaperrc.");
        config.database =
            shellexpand::tilde_with_context(&config.database, dirs::home_dir).to_string();
        config
    };

    let db = taskpaper::Database::from_dir(&config.database).expect("Could not open the database.");

    let result = match args.cmd {
        Some(Command::Search(args)) => search::search(&db, &args),
        Some(Command::ToInbox(args)) => to_inbox::to_inbox(&db, &args),
        Some(Command::Format(args)) => format::format(&db, &args),
        Some(Command::Housekeeping(args)) => housekeeping::run(&db, &args),
        Some(Command::LogDone(args)) => log_done::run(&db, &args),
        Some(Command::PurgeTags(args)) => purge_tags::run(&db, &args),
        Some(Command::Filter(args)) => filter::run(&db, &args),
        Some(Command::CheckFeeds(args)) => check_feeds::run(&db, &args, &config),
        None => {
            // TODO(sirver): I found no easy way to make clap output the usage here.
            println!("Need a subcommand.");
            std::process::exit(1);
        }
    };
    if let Err(err) = result {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}
