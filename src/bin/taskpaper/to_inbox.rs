#[cfg(target_os = "macos")]
use clipboard::{ClipboardContext, ClipboardProvider};
#[cfg(target_os = "macos")]
use osascript::JavaScript;
use soup::{NodeExt, Soup};
use std::io::{self, BufRead};
use structopt::StructOpt;
use taskpaper::tag;
use taskpaper::{Error, Result};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// Add a link to the currently selected mail message to the item.
    #[structopt(short = "-m", long = "--mail")]
    mail: bool,

    /// Prompt for input instead of reading stdout directly.
    #[structopt(short = "-p", long = "--prompt")]
    prompt: bool,

    /// Assume the input text is base64 encoded and decode it first.
    #[structopt(long = "--base64")]
    base64: bool,
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
        '.' => {
            command.arg("-selection").arg("c");
        }
        _ => unreachable!(),
    }
    let output = command.output()?;
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

    for mut untrimmed_line in lines {
        let mut line_with_tags = untrimmed_line.trim().to_string();

        if args.base64 {
            let decoded = base64::decode(&line_with_tags).map_err(|_| {
                Error::misc("Input not base64 encoded, though base64 decoding was requested.")
            })?;
            untrimmed_line = String::from_utf8_lossy(&decoded).to_string();
            line_with_tags = untrimmed_line.trim().to_string();
        }

        let mut note_text = Vec::new();
        let (mut line_without_tags, tags) = tag::extract_tags(line_with_tags);
        if line_without_tags.starts_with("http") {
            let _ = reqwest::Client::builder()
                .redirect(reqwest::RedirectPolicy::limited(10))
                .build()
                .map(|client| {
                    client
                        .get(&line_without_tags)
                        .send()
                        .ok()
                        .and_then(|resp| Soup::from_reader(resp).ok())
                        .map(|soup| {
                            let mut description_texts = Vec::new();
                            // Find and push the title.
                            soup.tag("title").find().map(|node| {
                                let text = node.text().trim().to_string();
                                if !text.is_empty() {
                                    description_texts.push(text);
                                }
                            });
                            let mut extra_notes = Vec::new();
                            // Find and push the description.
                            soup.tag("meta")
                                .attr("name", "description")
                                .find()
                                .map(|node| {
                                    node.attrs().get("content").map(|t| match t.len() {
                                        0 => (),
                                        1...100 => description_texts.push(t.to_string()),
                                        _ => {
                                            extra_notes.extend(
                                                textwrap::wrap(t, 80)
                                                    .into_iter()
                                                    .map(|l| l.to_string()),
                                            );
                                        }
                                    });
                                });
                            if !description_texts.is_empty() {
                                note_text.push(line_without_tags.clone());
                                note_text.extend(extra_notes.into_iter());
                                line_without_tags = description_texts.join(" • ");
                            }
                        });
                });
        }

        if line_without_tags.starts_with(".") || line_without_tags.starts_with(",") {
            let clipboard = get_clipboard(line_without_tags.chars().next().unwrap())?;
            line_without_tags = line_without_tags[1..].trim().to_string();
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
            line_index: None,
            text: line_without_tags,
            tags: tags,
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
                    top_level: 0,
                    first_level: 0,
                    others: 0,
                },
                ..Default::default()
            },
        )
        .expect("Writing Inbox failed");
    Ok(())
}
