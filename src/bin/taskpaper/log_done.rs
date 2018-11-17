use chrono::NaiveDate;
use crate::ConfigurationFile;
use lazy_static::lazy_static;
use structopt::StructOpt;
use taskpaper::Error;
use taskpaper::{Entry, Result, Tag, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

fn log_to_logbook(done: Vec<Entry>, logbook: &mut TaskpaperFile) {
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
}

fn move_repeated_items_to_tickle(repeat: Vec<Entry>, tickle: &mut TaskpaperFile) -> Result<()> {
    for mut e in repeat {
        let tags = match e {
            Entry::Task(ref mut t) => &mut t.tags,
            Entry::Project(ref mut p) => &mut p.tags,
            Entry::Note(_) => unreachable!(),
        };
        let done_tag = &tags.get("done").unwrap().value.unwrap();
        let done_date = chrono::NaiveDate::parse_from_str(done_tag, "%Y-%m-%d")
            .map_err(|_| Error::misc(format!("Invalid date: {}", done_tag)))?;
        tags.remove("done");

        let duration = tags
            .get("repeat")
            .unwrap()
            .value
            .ok_or_else(|| Error::misc("Invalid @repeat without value."))
            .and_then(|v| parse_duration(&v))?;
        let to_inbox = (done_date + duration).format("%Y-%m-%d").to_string();
        tags.insert(Tag {
            name: "to_inbox".to_string(),
            value: Some(to_inbox),
        });
        tickle.entries.push(e);
    }
    tickle.entries.sort_by_key(|e| match e {
        Entry::Project(p) => p.tags.get("to_inbox").unwrap().value.unwrap(),
        Entry::Task(t) => t.tags.get("to_inbox").unwrap().value.unwrap(),
        Entry::Note(_) => unreachable!(),
    });
    Ok(())
}

pub fn run(_: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let mut todo = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Todo)?;
    let mut tickle = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Tickle)?;
    let mut logbook = TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Logbook)?;

    // TODO(sirver): This method could be much simpler expressed using the .filter() method.
    fn recurse(
        parent_texts: &[String],
        entries: Vec<Entry>,
        done: &mut Vec<Entry>,
        repeat: &mut Vec<Entry>,
    ) -> Vec<Entry> {
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
                        if p.tags.get("repeat").is_some() {
                            repeat.push(Entry::Project(p.clone()));
                        }
                        p.text = format!("{} • {}", parent_texts.join(" • "), p.text);
                        done.push(Entry::Project(p));
                    } else {
                        let mut parent_texts = parent_texts.to_vec();
                        parent_texts.push(p.text.to_string());
                        p.children = recurse(&parent_texts, p.children, done, repeat);
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
                        if t.tags.get("repeat").is_some() {
                            repeat.push(Entry::Task(t.clone()));
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
    let mut repeat = Vec::new();
    todo.entries = recurse(&Vec::new(), todo.entries, &mut done, &mut repeat);

    move_repeated_items_to_tickle(repeat, &mut tickle)?;
    log_to_logbook(done, &mut logbook);

    todo.overwrite_common_file(taskpaper::CommonFileKind::Todo, config.formats["todo"])?;
    logbook.overwrite_common_file(
        taskpaper::CommonFileKind::Logbook,
        config.formats["logbook"],
    )?;
    tickle.overwrite_common_file(taskpaper::CommonFileKind::Tickle, config.formats["inbox"])?;
    Ok(())
}

pub fn parse_duration(s: &str) -> Result<chrono::Duration> {
    lazy_static! {
        static ref DURATION: regex::Regex = { regex::Regex::new(r"(\d+)([dwmy])").unwrap() };
    };

    let captures = DURATION
        .captures(&s)
        .ok_or_else(|| Error::misc(format!("Invalid duration: {}", s)))?;
    let num: i32 = captures.get(1).unwrap().as_str().parse().unwrap();
    const HOURS: u64 = 60 * 60;
    const DAYS: u64 = HOURS * 24;
    let time = match captures.get(2).unwrap().as_str() {
        "d" => std::time::Duration::from_secs(num as u64 * DAYS),
        "w" => std::time::Duration::from_secs(num as u64 * 7 * DAYS),
        "m" => std::time::Duration::from_secs(num as u64 * 30 * DAYS),
        "y" => std::time::Duration::from_secs(num as u64 * 365 * DAYS),
        _ => unreachable!(),
    };
    Ok(chrono::Duration::from_std(time).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert!(parse_duration("trnae").is_err());
        assert_eq!(parse_duration("2w").unwrap(), chrono::Duration::weeks(2));
        assert_eq!(parse_duration("3m").unwrap(), chrono::Duration::days(90));
        assert_eq!(
            parse_duration("4y").unwrap(),
            chrono::Duration::days(4 * 365)
        );
    }
}
