//! CLI entry point

mod commands;

use clap::{Parser, Subcommand};
use commands::{Run, Serve, Tui, Acp};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rcode")]
#[command(version = "0.1.0")]
#[command(about = "AI coding agent", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    
    #[arg(short, long)]
    verbose: bool,
    
    /// Path to config file
    #[arg(long)]
    config: Option<PathBuf>,
    
    /// Disable config file loading
    #[arg(long)]
    no_config: bool,
}

#[derive(Subcommand)]
enum Commands {
    Run(Run),
    Serve(Serve),
    Tui(Tui),
    Acp(Acp),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    rcode_observability::init("cli", cli.verbose);
    
    match cli.command {
        Some(Commands::Run(run)) => run.execute(cli.config.as_ref(), cli.no_config).await,
        Some(Commands::Serve(serve)) => serve.execute(cli.config.as_ref(), cli.no_config).await,
        Some(Commands::Tui(tui)) => tui.execute(cli.config.as_ref(), cli.no_config).await,
        Some(Commands::Acp(acp)) => acp.execute(cli.config.as_ref(), cli.no_config).await,
        None => {
            let tui = Tui::default();
            tui.execute(cli.config.as_ref(), cli.no_config).await
        }
    }
}
