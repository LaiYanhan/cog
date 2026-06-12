mod analysis;
mod backup;
mod cli;
mod command;
mod domain;
mod experiment;
mod format;
mod repo;
mod space;
mod workflow;
use anyhow::{Context, Result};
use clap::Parser;

fn main() -> Result<()> {
    let cli = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            // Print the original Clap error (includes usage)
            e.print().ok();
            // If it's an unknown-argument error, add a concise hint for LLM agents
            let err_str = e.to_string();
            if err_str.contains("unexpected argument") || err_str.contains("unexpected subcommand")
            {
                eprintln!(
                    "\nNote: the flag you used does not exist. Run `cog <command> --help` to see available flags."
                );
            }
            std::process::exit(e.exit_code());
        }
    };
    // Determine DB path. Only `sync --init` or explicit `--db` can create a new DB.
    let db_path = if cli.is_sync_init() {
        let path = cli.init_db_path();
        // Create parent directory for init
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create cog directory: {}", parent.display()))?;
        }
        path
    } else if let Some(db) = cli.find_existing_db() {
        db
    } else if let Some(path) = cli.explicit_db() {
        // Explicit --db but file doesn't exist yet — allow creation (e.g. CI pipelines)
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create cog directory: {}", parent.display()))?;
        }
        path.clone()
    } else {
        anyhow::bail!(
            "No cognitive model found. Run `cog sync --init` to create one in the current directory, \
             or use `--db <path>` to specify a location."
        );
    };

    let store = repo::SqliteRepository::open(&db_path)?;
    let output = cli.run(&store)?;
    output.emit();

    if output.exit_code != 0 {
        std::process::exit(output.exit_code);
    }

    Ok(())
}
