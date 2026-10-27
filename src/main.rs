use mostro_client::cli::run;
use std::io::{self, Write};
use std::process;

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(code) => code,
        Err(e) => {
            // An unwritable stderr must not turn this error into a panic.
            let _ = writeln!(io::stderr(), "{e}");
            1
        }
    };
    process::exit(code);
}
