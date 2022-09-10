use anyhow::{anyhow, Result};
#[cfg(target_os = "macos")]
use copypasta::{ClipboardContext, ClipboardProvider};
#[cfg(target_os = "macos")]
use osascript::JavaScript;
use std::io::{self, BufRead};
use std::path::PathBuf;
use structopt::StructOpt;
use taskpaper::{sanitize_item_text, tag, Database, NodeId, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// Verbatim - ignore '.' and ',' for clipboard and do not expand urls.
    #[structopt(long = "--verbatim")]
    verbatim: bool,

    /// Add a link to the currently selected mail message to the item.
    #[structopt(short = "-m", long = "--mail")]
    mail: bool,

    /// Prompt for input instead of reading stdout directly.
    #[structopt(short = "-p", long = "--prompt")]
    prompt: bool,

    /// Assume the input text is base64 encoded and decode it first.
    #[structopt(long = "--base64")]
    base64: bool,

    /// Style to format with. The default is 'inbox'.
    #[structopt(short = "-s", long = "--style", default_value = "inbox")]
    style: String,

    /// The file to add this to. If not specified this is by default the inbox file.
    #[structopt(parse(from_os_str), short = "-f")]
    file: Option<PathBuf>,

    /// The project to add this item to. If empty, it will be added to the items of the file.
    #[structopt(long = "--project")]
    project: Option<String>,

    /// Prepend the new item (instead of appending it)
    #[structopt(long = "--prepend")]
    prepend: bool,

    /// Tags to add to this item (including @).
    #[structopt(long = "--tag")]
    tags: Vec<String>,
}

#[cfg(target_os = "macos")]
fn get_currently_selected_mail_message() -> Result<String> {
    let script = JavaScript::new(
        "
        var Mail = Application('Mail');
        return Mail.selection()[0].messageId()
    ",
    );
    let message_id = script.execute()?;
    Ok(message_id)
}

#[cfg(target_os = "macos")]
fn get_clipboard(_: char) -> Result<String> {
    let mut ctx = ClipboardContext::new()
        .map_err(|e| anyhow!("Could not create clipboard context: {}", e))?;
    let contents = ctx
        .get_contents()
        .map_err(|e| anyhow!("Could not get clipboard contents: {}", e))?;
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

pub fn parse_and_push_task(
    tpf: &mut TaskpaperFile,
    position: taskpaper::Position,
    mut line: String,
    base64: bool,
    verbatim: bool,
    mail: bool,
    additional_tags: &[String],
) -> Result<()> {
    let mut line_with_tags = line.trim().to_string();

    if base64 {
        let decoded = base64::decode(&line_with_tags)?;
        line = String::from_utf8_lossy(&decoded).to_string();
        line_with_tags = line.trim().to_string();
    }

    for t in additional_tags {
        line_with_tags.push(' ');
        line_with_tags.push_str(t);
    }

    let mut note_text = Vec::new();
    let (mut line_without_tags, tags) = tag::extract_tags(line_with_tags);

    if !verbatim {
        if let Ok(Some(summary)) = crate::check_feeds::get_summary_blocking(&line_without_tags) {
            note_text.extend(summary.note_text.into_iter());
            line_without_tags = summary.title;
        }

        if line_without_tags.starts_with('.') || line_without_tags.starts_with(',') {
            let clipboard = get_clipboard(line_without_tags.chars().next().unwrap())?;
            line_without_tags = line_without_tags[1..].trim().to_string();
            note_text.push(clipboard.trim().to_string());
        }
    }

    #[cfg(target_os = "macos")]
    {
        if mail {
            let mail_message =
                format!("message://%3C{}%3E", get_currently_selected_mail_message()?);
            note_text.push(mail_message);
        }
    }

    let text = sanitize_item_text(&line_without_tags);
    let node_id = tpf.insert(
        taskpaper::Item::new_with_tags(taskpaper::ItemKind::Task, text, tags),
        position,
    );

    for line in note_text {
        let text = sanitize_item_text(&line);
        tpf.insert(
            taskpaper::Item::new(taskpaper::ItemKind::Note, text),
            taskpaper::Position::AsLastChildOf(&node_id),
        );
    }
    Ok(())
}

fn find_project(tpf: &TaskpaperFile, text: &str) -> Option<NodeId> {
    tpf.iter()
        .filter(|n| n.item().is_project())
        .find(|n| n.item().text() == text)
        .map(|n| n.id().clone())
}

pub fn to_inbox(db: &Database, args: &CommandLineArguments) -> Result<()> {
    let config = db.config()?;
    let mut tpf = match &args.file {
        Some(f) => {
            if f.exists() {
                taskpaper::TaskpaperFile::parse_file(f)?
            } else {
                taskpaper::TaskpaperFile::new()
            }
        }
        None => db.parse_common_file(taskpaper::CommonFileKind::Inbox)?,
    };

    let node_id;
    let position = match &args.project {
        Some(p) => {
            node_id =
                find_project(&tpf, p).ok_or_else(|| anyhow!("Could not find project '{}'.", p))?;
            if args.prepend {
                taskpaper::Position::AsFirstChildOf(&node_id)
            } else {
                taskpaper::Position::AsLastChildOf(&node_id)
            }
        }
        None => {
            if args.prepend {
                taskpaper::Position::AsFirst
            } else {
                taskpaper::Position::AsLast
            }
        }
    };

    let style = match config.formats.get(&args.style) {
        Some(format) => *format,
        None => return Err(anyhow!("Style '{}' not found.", args.style)),
    };

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
        parse_and_push_task(
            &mut tpf,
            position,
            line,
            args.base64,
            args.verbatim,
            args.mail,
            &args.tags,
        )?;
    }

    match &args.file {
        Some(f) => tpf.write(f, style)?,
        None => db.overwrite_common_file(&tpf, taskpaper::CommonFileKind::Inbox)?,
    };
    Ok(())
}
