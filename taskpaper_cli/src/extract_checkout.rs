use taskpaper::{Database, Level, Position, Result, TaskpaperFile};

pub fn extract_checkout(db: &Database, todo: &mut TaskpaperFile) -> Result<()> {
    if let Some(path) = db.path_of_common_file(taskpaper::CommonFileKind::Checkout) {
        taskpaper::mirror_changes(&path, todo)?;
    }
    let mut checkout = TaskpaperFile::new();

    const PROJECTS: [(&str, &str); 6] = [
        ("Reading", "@reading and not @done and not @req_login"),
        ("Watching", "@watching and not @done and not @req_login"),
        ("Listening", "@listening and not @done and not @req_login"),
        ("Arbeit • Reading", "@reading and not @done and @req_login"),
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
        let search_results = todo.search(query)?;
        if search_results.is_empty() {
            continue;
        }

        let project_id = checkout.insert(
            taskpaper::Item {
                kind: taskpaper::ItemKind::Project,
                line_index: None,
                text: title.to_string(),
                tags: taskpaper::Tags::new(),
            },
            Level::Top,
            Position::AsLast,
        );
        for node_id in &search_results {
            let task_id = checkout.insert(
                todo[node_id].item().clone(),
                Level::Under(&project_id),
                Position::AsLast,
            );

            // Also copy over the notes that are immediate children.
            for c in todo[node_id].children() {
                if !todo[&c].item().is_note() {
                    continue;
                }
                checkout.insert(
                    todo[&c].item().clone(),
                    Level::Under(&task_id),
                    Position::AsLast,
                );
            }
        }
    }

    db.overwrite_common_file(
        &checkout,
        taskpaper::CommonFileKind::Checkout,
        taskpaper::FormatOptions::default(),
    )?;
    Ok(())
}
