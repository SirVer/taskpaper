use crate::ConfigurationFile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use taskpaper::{db::Database, Entry, Result, TaskpaperFile, ToStringWithIndent};

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

    // TODO(sirver): Retain line number for entries.
    /// Print location (filename) for the match.
    #[structopt(short = "-l")]
    location: bool,
}

pub fn search(args: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
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
    if let Some(path) = &args.input {
        let taskpaper_file = TaskpaperFile::parse_file(&path).unwrap();
        files.push((path.to_owned(), taskpaper_file));
    } else {
        let db = Database::read(&config.database)?;
        for (path, tpf) in db.files {
            files.push((path, tpf));
        }
    }

    let mut results: HashMap<&Path, _> = HashMap::new();
    for (path, tpf) in &files {
        results.insert(path as &Path, tpf.search(&query)?);
    }

    let print_results = |results: &[&Entry], indent| {
        print!(
            "{}",
            results.to_string(
                indent,
                taskpaper::FormatOptions {
                    sort: taskpaper::Sort::Nothing,
                    print_children: if args.descendants {
                        taskpaper::PrintChildren::Yes
                    } else {
                        taskpaper::PrintChildren::No
                    },
                    print_notes: if args.descendants {
                        taskpaper::PrintNotes::Yes
                    } else {
                        taskpaper::PrintNotes::No
                    },
                    ..Default::default()
                }
            )
        );
    };

    if results.len() == 1 {
        print_results(&results.into_iter().next().unwrap().1, 0)
    } else {
        let mut files: Vec<_> = results.keys().collect();
        files.sort();
        for f in files {
            if results[f].is_empty() {
                continue;
            }
            println!("{}:", f.display());
            print_results(&results[f], 1)
        }
    }

    Ok(())
}
