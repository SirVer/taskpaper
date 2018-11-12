use chrono::NaiveDate;
use crate::ConfigurationFile;
use structopt::StructOpt;
use taskpaper::{Entry, Result, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

pub fn run(_: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let mut todo = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Todo)?;

    // TODO(sirver): This method could be much simpler expressed using the .filter() method.
    fn recurse(parent_texts: &[String], entries: Vec<Entry>, done: &mut Vec<Entry>) -> Vec<Entry> {
        let today = chrono::Local::now().date().format("%Y-%m-%d").to_string();
        let mut new_entries = Vec::new();
        for e in entries {
            match e {
                Entry::Project(mut p) => {
                    if p.tags.contains("done") {
                        let mut tag = p.tags.get("done").unwrap();
                        if tag.value.is_none() {
                            tag.value = Some(today.clone());
                            p.tags.insert(tag);
                        }
                        p.text = format!("{} • {}", parent_texts.join(" • "), p.text);
                        done.push(Entry::Project(p));
                    } else {
                        let mut parent_texts = parent_texts.to_vec();
                        parent_texts.push(p.text.to_string());
                        p.children = recurse(&parent_texts, p.children, done);
                        new_entries.push(Entry::Project(p));
                    }
                }
                Entry::Task(mut t) => {
                    if t.tags.contains("done") {
                        let mut tag = t.tags.get("done").unwrap();
                        if tag.value.is_none() {
                            tag.value = Some(today.clone());
                            t.tags.insert(tag);
                        }
                        t.text = format!("{} • {}", parent_texts.join(" • "), t.text);
                        done.push(Entry::Task(t));
                    } else {
                        new_entries.push(Entry::Task(t));
                    }
                }
                Entry::Note(n) => new_entries.push(Entry::Note(n)),
            }
        }
        new_entries
    }

    let mut done = Vec::new();
    todo.entries = recurse(&Vec::new(), todo.entries, &mut done);

    let mut logbook = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Logbook)?;

    for entry in done {
        let done = match &entry {
            Entry::Task(t) => t.tags.get("done").unwrap(),
            Entry::Project(p) => p.tags.get("done").unwrap(),
            Entry::Note(_) => unreachable!(),
        };
        let parent_project = NaiveDate::parse_from_str(done.value.as_ref().unwrap(), "%Y-%m-%d")
            .unwrap()
            .format("%A, %d. %B %Y")
            .to_string();
        let project = match logbook.get_project_mut(&parent_project) {
            Some(p) => p,
            None => {
                logbook.entries.push(Entry::Project(taskpaper::Project {
                    text: parent_project.to_string(),
                    note: None,
                    tags: taskpaper::Tags::new(),
                    children: Vec::new(),
                }));
                logbook.get_project_mut(&parent_project).unwrap()
            }
        };
        project.children.push(entry);
    }
    logbook.entries.sort_by_key(|e| match e {
        Entry::Project(p) => match NaiveDate::parse_from_str(&p.text, "%A, %d. %B %Y") {
            Ok(v) => v,
            Err(_) => panic!("Encountered unexpected date formatting: {}", p.text),
        },
        _ => panic!("Only expected projects!"),
    });
    logbook.entries.reverse();

    todo.overwrite_common_file(taskpaper::CommonFileKind::Todo, config.formats["todo"])?;
    logbook.overwrite_common_file(
        taskpaper::CommonFileKind::Logbook,
        config.formats["logbook"],
    )?;
    Ok(())
}
