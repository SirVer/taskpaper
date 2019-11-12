use taskpaper::{Entry, Error, Result, TaskpaperFile};

pub fn tickle(
    inbox: &mut TaskpaperFile,
    todo: &mut TaskpaperFile,
    tickle: &mut TaskpaperFile,
) -> Result<()> {
    // TODO(sirver): Maybe support 'every' tag, whenever we put something into the inbox from the
    // tickle file, we readd it in. This is similar to 'repeat' implemented in 'log_done'.
    // Remove tickle items from todo and inbox and add them to tickle.
    let mut entries = Vec::new();
    entries.append(&mut inbox.filter("@tickle")?);
    entries.append(&mut todo.filter("@tickle")?);
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
    Ok(())
}
