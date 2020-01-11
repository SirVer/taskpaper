use crate::ConfigurationFile;
use std::collections::BTreeMap;
use taskpaper::{Database, Error, Result, TaskpaperFile};

pub fn extract_timeline(
    db: &Database,
    todo: &mut TaskpaperFile,
    config: &ConfigurationFile,
) -> Result<()> {
    if let Some(path) = db.path_of_common_file(taskpaper::CommonFileKind::Timeline) {
        taskpaper::mirror_changes(&path, todo)?;
    }
    let today = chrono::Local::now().naive_local().date();
    let mut timeline = TaskpaperFile::new();
    let items = todo.search("@due and not @done")?;
    let mut sorted = BTreeMap::new();
    for mut item in items.into_iter().cloned() {
        let tags = match item {
            taskpaper::Item::Task(ref t) => &t.tags,
            taskpaper::Item::Project(ref mut p) => {
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
        sorted.entry(due).or_insert(Vec::new()).push(item);
    }

    for (due, due_items) in sorted {
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
            .items
            .push(taskpaper::Item::Project(taskpaper::Project {
                line_index: None,
                text: title.to_string(),
                note: None,
                tags: taskpaper::Tags::new(),
                children: due_items,
            }));
    }
    db.overwrite_common_file(
        &timeline,
        taskpaper::CommonFileKind::Timeline,
        config.formats["timeline"],
    )?;
    Ok(())
}
