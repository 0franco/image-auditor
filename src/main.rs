mod scanner;
mod tui;
mod config;
mod llm;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "image-auditor", version = "0.1.0", author, about)]
struct Cli {
    /// Path to scan
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    // Load environment variables from a local .env file if present.
    let _ = dotenvy::from_filename(".env");

    let cli = Cli::parse();
    tui::run(cli.path)?;
    Ok(())
}
