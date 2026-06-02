mod cli;
mod command;
mod format;
mod model;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let store = model::Store::open(&cli.db_path())?;
    let output = cli.run(&store)?;
    output.emit();

    if output.exit_code != 0 {
        std::process::exit(output.exit_code);
    }

    Ok(())
}
