use structopt::StructOpt;
use taskpaper::{Result, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

pub fn run(_: &CommandLineArguments) -> Result<()> {
    let todo = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Todo)?;

    let mut checkout = TaskpaperFile::new();

    const PROJECTS: [(&str, &str); 6] = [
        ("Reading", "@reading and not @done and not @req_login"),
        ("Watching", "@watching and not @done and not @req_login"),
        ("Listening", "@listening and not @done and not @req_login"),
        (
            "Arbeit • Reading",
            "@reading and not @done and @req_login",
        ),
        (
            "Arbeit • Watching",
            "@watching and not @done and @req_login",
        ),
        (
            "Arbeit • Listening",
            "@listening and not @done and @req_login",
        ),
    ];

    for (title, query) in PROJECTS.iter() {
        let entries = todo.search(query)?;
        if entries.is_empty() {
            continue;
        }

        checkout
            .entries
            .push(taskpaper::Entry::Project(taskpaper::Project {
                text: title.to_string(),
                note: None,
                tags: taskpaper::Tags::new(),
                children: entries.iter().map(|e| (**e).clone()).collect(),
            }));
    }

    checkout.overwrite_common_file(
        taskpaper::CommonFileKind::Checkout,
        taskpaper::FormatOptions {
            vim_read_only: taskpaper::VimReadOnly::Yes,
            ..Default::default()
        },
    )?;
    Ok(())
}
