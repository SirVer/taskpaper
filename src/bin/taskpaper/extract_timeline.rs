use crate::ConfigurationFile;
use std::collections::BTreeMap;
use structopt::StructOpt;
use taskpaper::{Error, Result, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

pub fn run(_: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let todo = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Todo)?;

    let today = chrono::Local::now().naive_local().date();
    //.format("%Y-%m-%d").to_string();

    let mut timeline = TaskpaperFile::new();

    let entries = todo.search("@due")?;
    let mut sorted = BTreeMap::new();
    for mut entry in entries.into_iter().cloned() {
        let tags = match entry {
            taskpaper::Entry::Note(_) => continue,
            taskpaper::Entry::Task(ref t) => &t.tags,
            taskpaper::Entry::Project(ref mut p) => {
                // We only want to print the due item, not their children.
                p.children.clear();
                &p.tags
            }
        };
        let due = tags.get("due").unwrap().value;
        if due.is_none() {
            continue;
        }
        let due = due.unwrap();
        let mut due = chrono::NaiveDate::parse_from_str(&due, "%Y-%m-%d")
            .map_err(|_| Error::misc(format!("Invalid date: {}", due)))?;
        if due < today {
            due = today.pred();
        }
        sorted.entry(due).or_insert(Vec::new()).push(entry);
    }

    for (due, due_entries) in sorted {
        let diff_days = due.signed_duration_since(today).num_days();
        let title = match diff_days {
            0 => "Today".to_string(),
            t if t < 0 => "Overdue".to_string(),
            _ => format!(
                "{} (+{} day{})",
                due.format("%A, %d. %B %Y"),
                diff_days,
                if diff_days != 1 { "s" } else { "" }
            ),
        };

        timeline
            .entries
            .push(taskpaper::Entry::Project(taskpaper::Project {
                text: title.to_string(),
                note: None,
                tags: taskpaper::Tags::new(),
                children: due_entries,
            }));
    }
    timeline.overwrite_common_file(
        taskpaper::CommonFileKind::Timeline,
        config.formats["timeline"],
    )?;
    Ok(())
}
