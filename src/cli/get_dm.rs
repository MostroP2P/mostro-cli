use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;

use crate::{
    db::User,
    parser::dms::{parse_dm_events, print_direct_messages},
    util::{create_filter, ListKind},
};

pub async fn execute_get_dm(
    _since: &i64,
    trade_index: i64,
    mostro_keys: &Keys,
    client: &Client,
    admin: bool,
    pool: &SqlitePool,
) -> Result<()> {
    let mut dm: Vec<(Message, u64, PublicKey)> = Vec::new();
    if !admin {
        for index in 1..=trade_index {
            let keys = User::get_trade_keys(pool, index).await?;
            let filter = create_filter(ListKind::DirectMessagesUser, keys.public_key());
            let fetched_events = client
                .fetch_events(filter, std::time::Duration::from_secs(15))
                .await?;
            let dm_temp = parse_dm_events(fetched_events, &keys).await;
            dm.extend(dm_temp);
        }
    } else {
        let filter = create_filter(ListKind::DirectMessagesMostro, mostro_keys.public_key());
        let fetched_events = client
            .fetch_events(filter, std::time::Duration::from_secs(15))
            .await?;
        let dm_temp = parse_dm_events(fetched_events, mostro_keys).await;
        dm.extend(dm_temp);
    }

    print_direct_messages(&dm, pool).await?;
    Ok(())
}
