use taskpaper::{Result, TaskpaperFile};

pub fn extract_checkout(todo: &mut TaskpaperFile) -> Result<()> {
    if let Some(path) = taskpaper::CommonFileKind::Checkout.find() {
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
        let entries = todo.search(query)?;
        if entries.is_empty() {
            continue;
        }

        checkout
            .entries
            .push(taskpaper::Entry::Project(taskpaper::Project {
                line_index: None,
                text: title.to_string(),
                note: None,
                tags: taskpaper::Tags::new(),
                children: entries.iter().map(|e| (**e).clone()).collect(),
            }));
    }

    checkout.overwrite_common_file(
        taskpaper::CommonFileKind::Checkout,
        taskpaper::FormatOptions::default(),
    )?;
    Ok(())
}
