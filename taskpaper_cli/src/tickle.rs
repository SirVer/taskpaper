use anyhow::{anyhow, Result};
use taskpaper::{Level, Position, TaskpaperFile};

pub fn tickle(
    inbox: &mut TaskpaperFile,
    todo: &mut TaskpaperFile,
    tickle: &mut TaskpaperFile,
) -> Result<()> {
    // TODO(sirver): Maybe support 'every' tag, whenever we put something into the inbox from the
    // tickle file, we readd it in. This is similar to 'repeat' implemented in 'log_done'.
    // Remove tickle items from todo and inbox and add them to tickle.

    let mut node_ids = Vec::new();
    for node_id in inbox.filter("@tickle")? {
        node_ids.push(tickle.copy_node(inbox, &node_id));
    }
    for node_id in todo.filter("@tickle")? {
        node_ids.push(tickle.copy_node(todo, &node_id));
    }

    for node_id in node_ids {
        let tags = tickle[&node_id].item_mut().tags_mut();
        let mut tag = tags.get("tickle").unwrap();
        if tag.value.is_none() {
            return Err(anyhow!(
                "Found @tickle without value: {:?}",
                tickle[&node_id].item()
            ));
        }
        tag.name = "to_inbox".to_string();
        tags.remove("tickle");
        tags.insert(tag);
        tickle.insert_node(node_id, Level::Top, Position::AsLast);
    }
    tickle.sort_nodes_by_key(|node| node.item().tags().get("to_inbox").unwrap().value.unwrap());

    // Remove tickle items from tickle file and add to inbox.
    let today = chrono::Local::now().date();
    let to_inbox = tickle.filter(&format!(
        "@to_inbox <= \"{}\"",
        today.format("%Y-%m-%d").to_string()
    ))?;

    for node_id in to_inbox {
        let inbox_id = inbox.copy_node(tickle, &node_id);
        inbox.insert_node(inbox_id, Level::Top, Position::AsLast);
    }

    Ok(())
}
