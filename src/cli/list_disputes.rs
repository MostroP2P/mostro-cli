use anyhow::Result;

use crate::cli::Context;
use crate::parser::disputes::print_disputes_table;
use crate::util::{fetch_events_list, ListKind};

pub async fn execute_list_disputes(ctx: &Context) -> Result<()> {
    // Print mostro pubkey
    println!(
        "Requesting disputes from mostro pubId - {}",
        &ctx.mostro_pubkey
    );

    // Get orders from relays
    let table_of_disputes =
        fetch_events_list(ListKind::Disputes, None, None, None, ctx, None, None).await?;
    let table = print_disputes_table(table_of_disputes)?;
    println!("{table}");

    Ok(())
}
