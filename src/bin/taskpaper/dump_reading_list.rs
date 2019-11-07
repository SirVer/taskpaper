use chrono::prelude::*;
use plist::serde::deserialize;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use structopt::StructOpt;
use taskpaper::Tags;

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
struct ReadingListEntry {
    date_added: DateTime<Utc>,
    date_last_viewed: Option<DateTime<Utc>>,
    preview_text: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Entry {
    title: Option<String>,
    children: Option<Vec<Entry>>,
    #[serde(rename = "URLString")]
    url_string: Option<String>,
    reading_list: Option<ReadingListEntry>,
    #[serde(rename = "URIDictionary")]
    uri_dictionary: Option<HashMap<String, String>>,
}

pub fn dump_reading_list(args: &CommandLineArguments) {
    let home = dirs::home_dir().expect("HOME not set.");
    let file = File::open(&home.join("Library/Safari/Bookmarks.plist")).unwrap();
    let plist: Entry = deserialize(file).unwrap();
    let c = plist
        .children
        .unwrap()
        .into_iter()
        .filter(|c| c.title == Some("com.apple.ReadingList".to_string()))
        .next()
        .unwrap();

    let mut tpf = if args.inbox {
        Some(taskpaper::TaskpaperFile::parse_common_file(taskpaper::CommonFileKind::Inbox).unwrap())
    } else {
        None
    };

    let mut num_entries = 0;
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
        num_entries += 1;
        if args.inbox {
            let mut tags = Tags::new();
            tags.insert(taskpaper::Tag {
                name: "reading".to_string(),
                value: None,
            });
            tpf.as_mut()
                .unwrap()
                .push_back(taskpaper::Entry::Task(taskpaper::Task {
                    line_index: None,
                    tags,
                    text: title.trim().to_string(),
                    note: Some(url.trim().to_string()),
                }));
        } else {
            println!("- {} @reading{}\n\t{}", title.trim(), done_str, url.trim());
        }
    }

    tpf.map(|tpf| {
        tpf.overwrite_common_file(
            taskpaper::CommonFileKind::Inbox,
            taskpaper::FormatOptions {
                sort: taskpaper::Sort::Nothing,
                empty_line_after_project: taskpaper::EmptyLineAfterProject {
                    top_level: 0,
                    first_level: 0,
                    others: 0,
                },
                ..Default::default()
            },
        )
        .expect("Writing Inbox failed");
        println!("Wrote {} entries into Inbox!", num_entries);
    });
}
