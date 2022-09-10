use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
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

pub fn search(db: &Database, args: &CommandLineArguments) -> Result<()> {
    let config = db.config()?;
    let mut files = HashMap::new();
    if let Some(path) = &args.input {
        files.insert(
            path.to_path_buf(),
            TaskpaperFile::parse_file(&path).unwrap(),
        );
    } else {
        files = db.parse_all_files()?;
    }

    let matches = taskpaper::db::search(
        args.query.to_string(),
        args.sort_by.as_ref().map(|s| s as &str),
        &config,
        &files,
    )?;

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
