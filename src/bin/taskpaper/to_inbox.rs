#[cfg(target_os = "macos")]
use clipboard::{ClipboardContext, ClipboardProvider};
#[cfg(target_os = "macos")]
use osascript::JavaScript;
use std::io::{self, BufRead};
use structopt::StructOpt;
use taskpaper::Tags;
use taskpaper::{Error, Result};

/// Add items to the inbox.
///
/// This is smart about ',' and '.' as first entries to add a note with the contents of the
/// clipboard to every task that is added. Under Linux ',' is primary, i.e. the last mouse
/// selection, while '.' is the X11 clipboard (copy & pasted). There is no distinction under Mac OS
/// since there is only one clipboard.
#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// Add a link to the currently selected mail message to the item.
    #[structopt(short = "-m", long = "--mail")]
    mail: bool,

    #[structopt(short = "-p", long = "--prompt")]
    prompt: bool,
}

#[cfg(target_os = "macos")]
fn get_currently_selected_mail_message() -> Result<String> {
    let script = JavaScript::new(
        "
        var Mail = Application('Mail');
        return Mail.selection()[0].messageId()
    ",
    );
    let message_id = script.execute().map_err(|e| Error::misc(e.to_string()))?;
    Ok(message_id)
}

#[cfg(target_os = "macos")]
fn get_clipboard(_: char) -> Result<String> {
    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    let contents = ctx.get_contents()?;
    Ok(contents)
}

#[cfg(target_os = "linux")]
fn get_clipboard(which: char) -> Result<String> {
    use std::process::Command;

    let mut command = Command::new("xclip");
    command.arg("-o");
    match which {
        ',' => (),
        '.' => { command.arg("-selection").arg("c"); },
        _ => unreachable!(),
    }
    let output = command .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn to_inbox(args: &CommandLineArguments) -> Result<()> {
    let mut inbox = taskpaper::TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Inbox)
        .expect("Could not parse inbox");

    let input: Vec<String> = if args.prompt {
        let reply = rprompt::prompt_reply_stdout("Task> ")?;
        vec![reply]
    } else {
        let stdin = io::stdin();
        stdin
            .lock()
            .lines()
            .map(|e| e.unwrap_or_else(|_| "".into()))
            .collect()
    };

    let lines: Vec<String> = input.into_iter().filter(|l| !l.trim().is_empty()).collect();

    for line in lines {
        let mut l = line.trim();
        let mut note_text = Vec::new();
        if l.starts_with(".") || l.starts_with(",") {
            let clipboard = get_clipboard(l.chars().next().unwrap())?;
            l = l[1..].trim();
            note_text.push(clipboard.trim().to_string());
        }

        #[cfg(target_os = "macos")]
        {
            if args.mail {
                let mail_message =
                    format!("message://<{}>", get_currently_selected_mail_message()?);
                note_text.push(mail_message);
            }
        }

        inbox.push(taskpaper::Entry::Task(taskpaper::Task {
            text: l.to_string(),
            // TODO(sirver): Hackish - we trust that tags get passed through in 'text'.
            tags: Tags::new(),
            note: if note_text.is_empty() {
                None
            } else {
                Some(note_text.join("\n"))
            },
        }));
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
    Ok(())
}
