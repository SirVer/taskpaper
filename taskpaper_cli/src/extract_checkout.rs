use taskpaper::{Database, Result, TaskpaperFile};

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
        let items = todo.search(query)?;
        if items.is_empty() {
            continue;
        }

        checkout
            .items
            .push(taskpaper::Item::Project(taskpaper::Project {
                line_index: None,
                text: title.to_string(),
                note: None,
                tags: taskpaper::Tags::new(),
                children: items.iter().map(|e| (**e).clone()).collect(),
            }));
    }

    db.overwrite_common_file(
        &checkout,
        taskpaper::CommonFileKind::Checkout,
        taskpaper::FormatOptions::default(),
    )?;
    Ok(())
}
