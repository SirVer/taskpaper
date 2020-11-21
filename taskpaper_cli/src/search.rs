use crate::ConfigurationFile;
use anyhow::Result;
use std::cmp;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use taskpaper::{db::Database, TaskpaperFile};

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// File to read. Otherwise every file in the database is considered.
    #[structopt(parse(from_os_str), long = "--input", short = "-i")]
    input: Option<PathBuf>,

    /// Search query to run against the file.
    query: String,

    /// Print descendants (notes & children) for results.
    #[structopt(short = "-d")]
    descendants: bool,

    /// Sort order. This can be a comma separated list of tag names, optionally prepended by a - to
    /// inverse the ordering. They will be used as keys in order of appearance.
    #[structopt(short = "-s")]
    sort_by: Option<String>,

    // TODO(sirver): Retain line number for items.
    /// Print location (filename) for the match.
    #[structopt(short = "-l")]
    location: bool,
}

#[derive(Debug)]
enum SortDir {
    Desc,
    Asc,
}

#[derive(Debug)]
struct SortBy {
    key: String,
    dir: SortDir,
}

fn get_sort_values(
    tpf: &TaskpaperFile,
    node_id: &taskpaper::NodeId,
    sorting_set: &[SortBy],
) -> Vec<Option<String>> {
    let mut values = Vec::new();
    let tags = tpf[node_id].item().tags();
    for s in sorting_set {
        match tags.get(&s.key) {
            None => values.push(None),
            Some(t) => match &t.value {
                None => values.push(None),
                Some(v) => values.push(Some(v.to_string())),
            },
        }
    }
    values
}

pub fn search(
    db: &Database,
    args: &CommandLineArguments,
    config: &ConfigurationFile,
) -> Result<()> {
    let mut files = Vec::new();

    let mut query = args.query.clone();
    'outer: for _ in 0..50 {
        for (key, value) in &config.aliases {
            let new_query = query.replace(key, value);
            if new_query != query {
                query = new_query;
                continue 'outer;
            }
        }
    }

    let sort_order = args.sort_by.as_ref().map(|s| {
        let mut res = Vec::new();
        for entry in s.split(",") {
            let entry = entry.trim();
            if entry.starts_with("-") {
                res.push(SortBy {
                    key: entry.trim_start_matches("-").to_string(),
                    dir: SortDir::Desc,
                })
            } else {
                res.push(SortBy {
                    key: entry.to_string(),
                    dir: SortDir::Asc,
                })
            }
        }
        res
    });

    let single_file;
    let all_files;
    if let Some(path) = &args.input {
        single_file = Some(TaskpaperFile::parse_file(&path).unwrap());
        files.push((path, single_file.as_ref().unwrap()));
    } else {
        all_files = db.parse_all_files()?;
        for (path, tpf) in &all_files {
            files.push((path, tpf));
        }
    }

    let mut results: HashMap<&Path, _> = HashMap::new();
    for (path, tpf) in files {
        results.insert(path as &Path, (tpf.search(&query)?, tpf));
    }

    let mut paths: Vec<_> = results.keys().map(|s| s.to_path_buf()).collect();
    paths.sort();
    for path in paths {
        let (mut node_ids, tpf) = results.remove(&path as &Path).expect("By construction");
        if node_ids.is_empty() {
            continue;
        }

        if let Some(ref s) = sort_order {
            node_ids.sort_by(|a, b| {
                let val_a = get_sort_values(tpf, a, &s);
                let val_b = get_sort_values(tpf, b, &s);
                for (idx, s) in s.iter().enumerate() {
                    let res = match s.dir {
                        SortDir::Asc => val_a[idx].cmp(&val_b[idx]),
                        SortDir::Desc => val_b[idx].cmp(&val_a[idx]),
                    };
                    match res {
                        cmp::Ordering::Less | cmp::Ordering::Greater => return res,
                        cmp::Ordering::Equal => (),
                    }
                }
                cmp::Ordering::Equal
            });
        }

        for node_id in node_ids.iter() {
            let item = tpf[node_id].item();
            let line = item.line_index().unwrap() + 1;
            let text = tpf.node_to_string(node_id);
            print!("{}:{}:{}", path.display(), line, text);
            if args.descendants {
                // We skip the node itself, since that has been taken care off.
                for child_node in tpf.iter_node(node_id).skip(1) {
                    let indent = child_node.item().indent - item.indent;
                    let indent_str = "\t".repeat(indent as usize);
                    let text = tpf.node_to_string(child_node.id());
                    print!("{}{}", indent_str, text);
                }
            }
        }
    }

    Ok(())
}
