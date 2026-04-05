//! Acp command - ACP protocol server mode

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use rcode_acp::AcpServer;

#[derive(Args)]
pub struct Acp {
    #[arg(short, long)]
    verbose: bool,
}

#[allow(dead_code)]
impl Acp {
    pub async fn execute(&self, _config_path: Option<&PathBuf>, _no_config: bool) -> Result<()> {
        if self.verbose {
            tracing::subscriber::set_global_default(
                tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::DEBUG)
                    .finish(),
            )?;
        }

        tracing::info!("Starting ACP server (stdio mode)");
        
        let server = AcpServer::new();
        server.run().await?;

        Ok(())
    }
}
