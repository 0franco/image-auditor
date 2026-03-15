mod app;
mod config;
mod llm;
mod patch;
mod scanner;
mod tui;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "image-auditor", version = "0.2.0", author, about)]
struct Cli {
    /// Path to scan (defaults to current directory)
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let _ = dotenvy::from_filename(".env");
    let cli = Cli::parse();
    tui::run(cli.path)?;
    Ok(())
}
