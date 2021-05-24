use crate::ConfigurationFile;
use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use lazy_static::lazy_static;
use std::borrow::Cow;
use std::cmp;
use structopt::StructOpt;
use taskpaper::{ChildrenStrategy, Database, Item, NodeId, Position, Tag, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

fn find_project(tpf: &TaskpaperFile, text: &str) -> Option<NodeId> {
    tpf.iter()
        .filter(|node| node.item().is_project())
        .find(|node| node.item().text() == text)
        .map(|node| node.id().clone())
}

/// The items in 'done' are ordered, so that they can be processed in order and unlinked without
/// damaging the structure of 'todo'.
fn log_to_logbook(done: Vec<NodeId>, todo: &mut TaskpaperFile, logbook: &mut TaskpaperFile) {
    let today = chrono::Local::now().date().format("%Y-%m-%d").to_string();

    for source_node_id in done {
        let node_id = logbook.copy_node(todo, &source_node_id);
        let item = logbook[&node_id].item_mut();

        // Change the text of this item to contain all parents.
        let new_text = {
            let mut texts = Vec::new();
            let mut cur = Some(&source_node_id);
            while let Some(id) = cur {
                let node = &todo[id];
                texts.push(node.item().text());
                cur = node.parent();
            }
            texts.reverse();
            texts.join(" • ")
        };
        item.text = new_text;

        todo.unlink_node(source_node_id, ChildrenStrategy::Remove);

        // Find the name of the parent project in the logbook.
        let parent_project = {
            let tags = item.tags_mut();
            let mut tag = tags.get("done").unwrap();
            if tag.value.is_none() {
                tag.value = Some(today.clone());
                tags.insert(tag);
            }
            let done = item.tags().get("done").unwrap();
            NaiveDate::parse_from_str(done.value.as_ref().unwrap(), "%Y-%m-%d")
                .unwrap()
                .format("%A, %d. %B %Y")
                .to_string()
        };

        let project_id = match find_project(logbook, &parent_project) {
            Some(project_id) => project_id,
            None => logbook.insert(
                Item::new(taskpaper::ItemKind::Project, parent_project),
                Position::AsLast,
            ),
        };
        logbook.insert_node(node_id, Position::AsLastChildOf(&project_id));
    }
    logbook.sort_nodes_by_key(|node| {
        cmp::Reverse(
            match NaiveDate::parse_from_str(&node.item().text(), "%A, %d. %B %Y") {
                Ok(v) => v,
                Err(_) => panic!(
                    "Encountered unexpected date formatting: '{}'",
                    node.item().text()
                ),
            },
        )
    });
}

fn reset_boxes(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.trim_start().starts_with("[X]") {
                Cow::Owned(l.replacen("[X]", "[_]", 1))
            } else {
                Cow::Borrowed(l)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_repeated_items_to_tickle(
    repeated_items: &[NodeId],
    todo: &TaskpaperFile,
    tickle: &mut TaskpaperFile,
) -> Result<()> {
    for source_node_id in repeated_items {
        let node_id = tickle.copy_node(todo, source_node_id);
        tickle.insert_node(node_id.clone(), Position::AsLast);

        let item = tickle[&node_id].item_mut();
        let done_tag = item.tags().get("done").unwrap().value.unwrap();
        let done_date = chrono::NaiveDate::parse_from_str(&done_tag, "%Y-%m-%d")
            .with_context(|| format!("Invalid date: {}", done_tag))?;
        item.tags_mut().remove("done");

        let duration = item
            .tags()
            .get("repeat")
            .unwrap()
            .value
            .ok_or_else(|| anyhow!("Invalid @repeat without value."))
            .and_then(|v| parse_duration(&v))?;
        let to_inbox = (done_date + duration).format("%Y-%m-%d").to_string();
        item.tags_mut().insert(Tag {
            name: "to_inbox".to_string(),
            value: Some(to_inbox),
        });

        // Remove boxes [X] => [_]
        for mut node in tickle
            .iter_node_mut(&node_id)
            .filter(|n| n.item().is_note())
        {
            let text = reset_boxes(node.item().text());
            node.item_mut().text = text;
        }
    }
    tickle.sort_nodes_by_key(|node| node.item().tags().get("to_inbox").unwrap().value.unwrap());
    Ok(())
}

pub fn parse_duration(s: &str) -> Result<chrono::Duration> {
    lazy_static! {
        static ref DURATION: regex::Regex = regex::Regex::new(r"(\d+)([dwmy])").unwrap();
    };

    let captures = DURATION
        .captures(&s)
        .ok_or_else(|| anyhow!("Invalid duration: {}", s))?;
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

pub fn run(db: &Database, _: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let mut todo = db.parse_common_file(taskpaper::CommonFileKind::Todo)?;
    let mut tickle = db.parse_common_file(taskpaper::CommonFileKind::Tickle)?;
    let mut logbook = db.parse_common_file(taskpaper::CommonFileKind::Logbook)?;

    // Figure out the items we need to look at and sort them by deepest indent first. This allows
    // us to process (and unlink) them in order without changing the structure of the todo file.
    let mut done_items = Vec::new();
    let mut repeated_items = Vec::new();
    for node_id in todo.search("@done")? {
        let mut depth = 0;
        let mut cur = &node_id;
        while let Some(p) = todo[cur].parent() {
            depth += 1;
            cur = p;
        }
        done_items.push((-depth, node_id.clone()));
        if todo[&node_id].item().tags().get("repeat").is_some() {
            repeated_items.push(node_id);
        }
    }
    done_items.sort_by_key(|i| i.0);

    append_repeated_items_to_tickle(&repeated_items, &todo, &mut tickle)?;
    log_to_logbook(
        done_items.into_iter().map(|e| e.1).collect::<Vec<_>>(),
        &mut todo,
        &mut logbook,
    );

    todo.format(config.formats["todo"]);
    logbook.format(config.formats["logbook"]);

    db.overwrite_common_file(&todo, taskpaper::CommonFileKind::Todo)?;
    db.overwrite_common_file(&logbook, taskpaper::CommonFileKind::Logbook)?;
    db.overwrite_common_file(&tickle, taskpaper::CommonFileKind::Tickle)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskpaper::testing::*;

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

    #[test]
    fn test_log_done() {
        let mut test = DatabaseTest::new();
        let config: ConfigurationFile =
            toml::from_str(include_str!("tests/log_done/taskpaperrc")).unwrap();

        test.write_file(
            "02_todo.taskpaper",
            include_str!("tests/log_done/todo_in.taskpaper"),
        );
        test.write_file("40_logbook.taskpaper", "");
        test.write_file("03_tickle.taskpaper", "");

        let db = test.read_database();

        run(db, &CommandLineArguments {}, &config).unwrap();

        test.assert_eq_to_golden(
            "src/tests/log_done/tickle_out.taskpaper",
            "03_tickle.taskpaper",
        );
        test.assert_eq_to_golden(
            "src/tests/log_done/logbook_out.taskpaper",
            "40_logbook.taskpaper",
        );
    }
}
