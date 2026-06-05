mod analysis;
mod cli;
mod command;
mod domain;
mod format;
mod repo;
mod space;
mod workflow;
use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let store = repo::SqliteRepository::open(&cli.db_path())?;
    let output = cli.run(&store)?;
    output.emit();

    if output.exit_code != 0 {
        std::process::exit(output.exit_code);
    }

    Ok(())
}
