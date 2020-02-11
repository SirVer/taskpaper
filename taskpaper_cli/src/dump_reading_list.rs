use chrono::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use structopt::StructOpt;
use taskpaper::{Database, Position, Tags};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// Include done, i.e. read items. Otherwise they are ignored.
    #[structopt(long = "--done")]
    done: bool,

    /// Dump found items to the inbox document.
    #[structopt(short = "-i", long = "--inbox")]
    inbox: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct ReadingListItem {
    date_added: DateTime<Utc>,
    date_last_viewed: Option<DateTime<Utc>>,
    preview_text: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Item {
    title: Option<String>,
    children: Option<Vec<Item>>,
    #[serde(rename = "URLString")]
    url_string: Option<String>,
    reading_list: Option<ReadingListItem>,
    #[serde(rename = "URIDictionary")]
    uri_dictionary: Option<HashMap<String, String>>,
}

pub fn dump_reading_list(db: &Database, args: &CommandLineArguments) {
    let home = dirs::home_dir().expect("HOME not set.");
    let plist: Item = plist::from_file(&home.join("Library/Safari/Bookmarks.plist")).unwrap();
    let c = plist
        .children
        .unwrap()
        .into_iter()
        .find(|c| c.title == Some("com.apple.ReadingList".to_string()))
        .unwrap();

    let mut tpf = if args.inbox {
        Some(
            db.parse_common_file(taskpaper::CommonFileKind::Inbox)
                .unwrap(),
        )
    } else {
        None
    };

    let mut num_items = 0;
    for e in &c.children.unwrap() {
        let title = &e.uri_dictionary.as_ref().unwrap()["title"];
        let url = &e.url_string.as_ref().unwrap();
        let read = e.reading_list.as_ref().unwrap();
        let done_str = match read.date_last_viewed {
            None => "".to_string(),
            Some(d) => {
                let local = d.with_timezone(&Local);
                local.format(" @done(%Y-%m-%d)").to_string()
            }
        };
        if !done_str.is_empty() && !args.done {
            continue;
        }
        num_items += 1;
        if args.inbox {
            let mut tags = Tags::new();
            tags.insert(taskpaper::Tag {
                name: "reading".to_string(),
                value: None,
            });
            let node_id = tpf.as_mut().unwrap().insert(
                taskpaper::Item::new_with_tags(
                    taskpaper::ItemKind::Task,
                    title.trim().to_string(),
                    tags,
                ),
                Position::AsLast,
            );

            tpf.as_mut().unwrap().insert(
                taskpaper::Item::new(taskpaper::ItemKind::Note, url.trim().to_string()),
                Position::AsLastChildOf(&node_id),
            );
        } else {
            println!("- {} @reading{}\n\t{}", title.trim(), done_str, url.trim());
        }
    }

    if let Some(tpf) = tpf {
        db.overwrite_common_file(
            &tpf,
            taskpaper::CommonFileKind::Inbox,
            taskpaper::FormatOptions {
                sort: taskpaper::Sort::Nothing,
                empty_line_after_project: taskpaper::EmptyLineAfterProject {
                    top_level: 0,
                    first_level: 0,
                    others: 0,
                },
            },
        )
        .expect("Writing Inbox failed");
        println!("Wrote {} items into Inbox!", num_items);
    };
}
