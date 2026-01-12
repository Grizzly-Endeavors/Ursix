mod agent;
mod cli;
mod config;
mod llm;
mod tools;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
