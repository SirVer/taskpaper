use crate::ConfigurationFile;
use anyhow::Result;
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

    // TODO(sirver): Retain line number for items.
    /// Print location (filename) for the match.
    #[structopt(short = "-l")]
    location: bool,
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

    let mut paths: Vec<_> = results.keys().collect();
    paths.sort();
    for path in paths {
        let (node_ids, tpf) = &results[path];
        if node_ids.is_empty() {
            continue;
        }
        for node_id in node_ids {
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
