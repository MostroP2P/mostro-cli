use anyhow::Result;

use crate::cli::Context;
use crate::parser::common::{print_key_value, print_section_header};
use crate::parser::disputes::print_disputes_table;
use crate::util::{fetch_events_list, ListKind};

pub async fn execute_list_disputes(ctx: &Context) -> Result<()> {
    print_section_header("⚖️  List Disputes");
    print_key_value("🎯", "Mostro PubKey", &ctx.mostro_pubkey.to_string());
    print_key_value("💡", "Action", "Fetching disputes from relays...");
    println!();

    // Get orders from relays
    let table_of_disputes =
        fetch_events_list(ListKind::Disputes, None, None, None, ctx, None).await?;
    let table = print_disputes_table(table_of_disputes)?;
    println!("{table}");

    Ok(())
}
