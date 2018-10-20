use osascript::JavaScript;
use structopt::StructOpt;
use taskpaper::Tags;

/// Commandline script to cleverly add items to the inbox.
#[derive(StructOpt, Debug)]
#[structopt(name = "2inbox")]
struct CommandLineArguments {
    /// Add a link to the currently selected mail message to the item.
    #[structopt(short = "-m", long = "--mail")]
    mail: bool,
}

#[cfg(target_os = "macos")]
fn get_currently_selected_mail_message() -> String {
    let script = JavaScript::new(
        "
        var Mail = Application('Mail');
        return Mail.selection()[0].messageId()
    ",
    );
    script.execute().unwrap()
}

fn main() {
    let args = CommandLineArguments::from_args();

    let mut inbox = taskpaper::TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Inbox)
        .expect("Could not parse inbox");

    #[cfg(target_os = "macos")]
    {
        if args.mail {
            inbox.push(taskpaper::Entry::Task(taskpaper::Task {
                tags: Tags::new(),
                text: "A message for you".to_string(),
                note: Some(format!(
                    "message://<{}>",
                    get_currently_selected_mail_message()
                )),
            }));
        }
    }

    inbox
        .overwrite_common_file(
            taskpaper::CommonFileKind::Inbox,
            taskpaper::FormatOptions {
                sort: taskpaper::Sort::Nothing,
                empty_line_after_project: taskpaper::EmptyLineAfterProject {
                    top_level: false,
                    first_level: false,
                    others: false,
                },
                ..Default::default()
            },
        )
        .expect("Writing Inbox failed");
}
