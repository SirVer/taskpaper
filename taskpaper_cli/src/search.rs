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
    sorting_set: &[SortBy],
    node_id: &taskpaper::NodeId,
    path: &Path,
    line_no: usize,
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
    // As tiebreaker, we use (path, string)
    values.push(Some(path.to_string_lossy().to_string()));
    values.push(Some(format!("{:05}", line_no)));
    values
}

pub fn search(db: &Database, args: &CommandLineArguments) -> Result<()> {
    let config = db.configuration()?;
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
            if let Some(name) = path.file_name() {
                if config
                    .search
                    .excluded_files
                    .contains(name.to_string_lossy().as_ref())
                {
                    continue;
                }
            }
            files.push((path, tpf));
        }
    }

    let mut searches: HashMap<&Path, _> = HashMap::new();
    struct Match<'a> {
        tpf: &'a TaskpaperFile,
        path: &'a Path,
        line_no: usize,
        node_id: taskpaper::NodeId,
    }

    for (path, tpf) in files {
        searches.insert(path as &Path, (tpf.search(&query)?, tpf));
    }

    let mut matches = Vec::new();
    for path in searches.keys() {
        let (node_ids, tpf) = &searches[&path as &Path];
        if node_ids.is_empty() {
            continue;
        }

        for node_id in node_ids.iter() {
            let item = tpf[node_id].item();
            matches.push(Match {
                tpf,
                path,
                line_no: item.line_index().unwrap() + 1,
                node_id: node_id.clone(),
            });
        }
    }

    if let Some(ref s) = sort_order {
        matches.sort_by(|a, b| {
            let val_a = get_sort_values(a.tpf, &s, &a.node_id, a.path, a.line_no);
            let val_b = get_sort_values(b.tpf, &s, &b.node_id, b.path, b.line_no);
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

    for m in matches {
        let item = m.tpf[&m.node_id].item();
        let line = item.line_index().unwrap() + 1;
        let text = m.tpf.node_to_string(&m.node_id);
        print!("{}:{}:{}", m.path.display(), line, text);
        if args.descendants {
            // We skip the node itself, since that has been taken care off.
            for child_node in m.tpf.iter_node(&m.node_id).skip(1) {
                let indent = child_node.item().indent - item.indent;
                let indent_str = "\t".repeat(indent as usize);
                let text = m.tpf.node_to_string(child_node.id());
                print!("{}{}", indent_str, text);
            }
        }
    }

    Ok(())
}
