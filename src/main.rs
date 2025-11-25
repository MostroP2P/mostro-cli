use anyhow::Result;
use mostro_client::cli::run;
use std::process;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = run().await {
        eprintln!("{e}");
        process::exit(1);
    }

    process::exit(0);
}
