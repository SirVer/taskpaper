use taskpaper::{Error, Item, Result, TaskpaperFile};

pub fn tickle(
    inbox: &mut TaskpaperFile,
    todo: &mut TaskpaperFile,
    tickle: &mut TaskpaperFile,
) -> Result<()> {
    // TODO(sirver): Maybe support 'every' tag, whenever we put something into the inbox from the
    // tickle file, we readd it in. This is similar to 'repeat' implemented in 'log_done'.
    // Remove tickle items from todo and inbox and add them to tickle.
    let mut items = Vec::new();
    items.append(&mut inbox.filter("@tickle")?);
    items.append(&mut todo.filter("@tickle")?);
    for mut item in items {
        let tags = match item {
            Item::Project(ref mut p) => Some(&mut p.tags),
            Item::Task(ref mut t) => Some(&mut t.tags),
        };

        if let Some(tags) = tags {
            let mut tag = tags.get("tickle").unwrap();
            if tag.value.is_none() {
                return Err(Error::misc(format!(
                    "Found @tickle without value: {:?}",
                    item
                )));
            }
            tag.name = "to_inbox".to_string();
            tags.remove("tickle");
            tags.insert(tag);
        }
        tickle.items.push(item);
    }
    tickle.items.sort_by_key(|item| match item {
        Item::Project(p) => p.tags.get("to_inbox").unwrap().value.unwrap(),
        Item::Task(t) => t.tags.get("to_inbox").unwrap().value.unwrap(),
    });

    // Remove tickle items from tickle file and add to inbox.
    let today = chrono::Local::now().date();
    let mut to_inbox = tickle.filter(&format!(
        "@to_inbox <= \"{}\"",
        today.format("%Y-%m-%d").to_string()
    ))?;
    inbox.items.append(&mut to_inbox);
    Ok(())
}
