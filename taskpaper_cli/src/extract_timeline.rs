use crate::ConfigurationFile;
use std::collections::BTreeMap;
use taskpaper::{Database, Error, Level, Position, Result, TaskpaperFile};

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
    let node_ids = todo.search("@due and not @done")?;
    let mut sorted = BTreeMap::new();
    for node_id in &node_ids {
        let item = todo[node_id].item();

        let due = match item.tags().get("due").unwrap().value {
            None => continue,
            Some(v) => v,
        };
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

        let project_id = timeline.insert(
            taskpaper::Item {
                kind: taskpaper::ItemKind::Project,
                line_index: None,
                text: title.to_string(),
                tags: taskpaper::Tags::new(),
            },
            Level::Top,
            Position::AsLast,
        );

        for item in due_items {
            // We do not copy over any notes here, just the item itself.
            timeline.insert(item.clone(), Level::Under(&project_id), Position::AsLast);
        }
    }
    db.overwrite_common_file(
        &timeline,
        taskpaper::CommonFileKind::Timeline,
        config.formats["timeline"],
    )?;
    Ok(())
}
