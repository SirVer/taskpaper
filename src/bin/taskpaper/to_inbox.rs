#[cfg(target_os = "macos")]
use osascript::JavaScript;

use std::io::{self, BufRead};
use structopt::StructOpt;
use taskpaper::Tags;

/// Commandline script to cleverly add items to the inbox.
#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// Add a link to the currently selected mail message to the item.
    #[structopt(short = "-m", long = "--mail")]
    mail: bool,
    // NOCOM(#sirver): missing: prompt, smart
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

pub fn to_inbox(args: &CommandLineArguments) {
    let mut inbox = taskpaper::TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Inbox)
        .expect("Could not parse inbox");

    let stdin = io::stdin();
    let lines: Vec<String> = stdin
        .lock()
        .lines()
        .map(|e| e.unwrap_or_else(|_| "".into()))
        .collect();

    for line in lines {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        inbox.push(taskpaper::Entry::Task(taskpaper::Task {
            text: l.to_string(),
            // TODO(sirver): Hackish - we trust that tags get passed through in 'text'.
            tags: Tags::new(),
            note: None,
        }));
    }

    // NOCOM(#sirver): this is not useful in its current form
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
