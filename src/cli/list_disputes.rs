use anyhow::Result;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::Client;

use crate::pretty_table::print_disputes_table;
use crate::util::get_disputes_list;

pub async fn execute_list_disputes(mostro_key: XOnlyPublicKey, client: &Client) -> Result<()> {
    println!(
        "Requesting orders from mostro pubId - {}",
        mostro_key.clone()
    );

    // Get orders from relays
    let table_of_disputes = get_disputes_list(mostro_key, client).await?;
    let table = print_disputes_table(table_of_disputes)?;
    println!("{table}");

    Ok(())
}
