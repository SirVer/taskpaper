use crate::ConfigurationFile;
use structopt::StructOpt;
use taskpaper::{Entry, Error, Result, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

pub fn run(_: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    // Remove tickle items from todo and inbox and add them to tickle.
    let mut entries = Vec::new();
    let mut inbox = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Inbox)?;
    entries.append(&mut inbox.filter("@tickle")?);

    let mut todo = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Todo)?;
    entries.append(&mut todo.filter("@tickle")?);

    let mut tickle = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Tickle)?;
    for mut e in entries {
        let tags = match e {
            Entry::Project(ref mut p) => Some(&mut p.tags),
            Entry::Task(ref mut t) => Some(&mut t.tags),
            Entry::Note(_) => None,
        };

        if let Some(tags) = tags {
            let mut tag = tags.get("tickle").unwrap();
            if tag.value.is_none() {
                return Err(Error::misc(format!("Found @tickle without value: {:?}", e)));
            }
            tag.name = "to_inbox".to_string();
            tags.remove("tickle");
            tags.insert(tag);
        }
        tickle.entries.push(e);
    }
    tickle.entries.sort_by_key(|e| match e {
        Entry::Project(p) => p.tags.get("to_inbox").unwrap().value.unwrap(),
        Entry::Task(t) => t.tags.get("to_inbox").unwrap().value.unwrap(),
        Entry::Note(_) => unreachable!(),
    });

    // Remove tickle items from tickle file and add to inbox.
    let today = chrono::Local::now().date();
    let mut to_inbox = tickle.filter(&format!(
        "@to_inbox <= \"{}\"",
        today.format("%Y-%m-%d").to_string()
    ))?;
    inbox.entries.append(&mut to_inbox);

    todo.overwrite_common_file(taskpaper::CommonFileKind::Todo, config.formats["todo"])?;
    inbox.overwrite_common_file(taskpaper::CommonFileKind::Inbox, config.formats["inbox"])?;
    tickle.overwrite_common_file(taskpaper::CommonFileKind::Tickle, config.formats["inbox"])?;
    Ok(())
}
