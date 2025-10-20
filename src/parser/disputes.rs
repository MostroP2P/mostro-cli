use anyhow::Result;
use chrono::DateTime;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use log::info;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

use crate::parser::common::{apply_status_color, create_error_cell};
use crate::util::Event;

use crate::nip33::dispute_from_tags;

pub fn parse_dispute_events(events: Events) -> Vec<Dispute> {
    // Extracted Disputes List
    let mut disputes_list = Vec::<Dispute>::new();

    // Scan events to extract all disputes
    for event in events.into_iter() {
        if let Ok(mut dispute) = dispute_from_tags(event.tags) {
            info!("Found Dispute id : {:?}", dispute.id);
            // Get created at field from Nostr event
            dispute.created_at = event.created_at.as_u64() as i64;
            disputes_list.push(dispute.clone());
        }
    }

    let buffer_dispute_list = disputes_list.clone();
    // Order all element ( orders ) received to filter - discard disaligned messages
    // if an order has an older message with the state we received is discarded for the latest one
    disputes_list.retain(|keep| {
        !buffer_dispute_list
            .iter()
            .any(|x| x.id == keep.id && x.created_at > keep.created_at)
    });

    // Sort by id to remove duplicates
    disputes_list.sort_by(|a, b| b.id.cmp(&a.id));
    disputes_list.dedup_by(|a, b| a.id == b.id);

    // Finally sort list by creation time
    disputes_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    disputes_list
}

pub fn print_disputes_table(disputes_table: Vec<Event>) -> Result<String> {
    // Convert Event to Dispute
    let disputes_table: Vec<Dispute> = disputes_table
        .into_iter()
        .filter_map(|event| {
            if let Event::Dispute(dispute) = event {
                Some(dispute)
            } else {
                None
            }
        })
        .collect();

    // Create table
    let mut table = Table::new();
    //Table rows
    let mut rows: Vec<Row> = Vec::new();

    if disputes_table.is_empty() {
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(160)
            .set_header(vec![Cell::new("📭 No Disputes")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center)]);

        // Single row for error
        let mut r = Row::new();

        r.add_cell(create_error_cell(
            "No disputes found with requested parameters…",
        ));

        //Push single error row
        rows.push(r);
    } else {
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(160)
            .set_header(vec![
                Cell::new("🆔 Dispute Id")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("📊 Status")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
                Cell::new("📅 Created")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center),
            ]);

        //Iterate to create table of orders
        for single_dispute in disputes_table.into_iter() {
            let date = DateTime::from_timestamp(single_dispute.created_at, 0);

            let status_str = single_dispute.status.to_string();
            let status_cell = apply_status_color(
                Cell::new(&status_str).set_alignment(CellAlignment::Center),
                &status_str,
            );

            let r = Row::from(vec![
                Cell::new(single_dispute.id).set_alignment(CellAlignment::Center),
                status_cell,
                Cell::new(
                    date.map(|d| d.to_string())
                        .unwrap_or_else(|| "Invalid date".to_string()),
                )
                .set_alignment(CellAlignment::Center),
            ]);
            rows.push(r);
        }
    }

    table.add_rows(rows);

    Ok(table.to_string())
}

#[cfg(test)]
mod tests {}
