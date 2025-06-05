use anyhow::{Ok, Result};
use mostro_core::order::Kind;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

pub fn order_from_tags(tags: Tags) -> Result<SmallOrder> {
    let mut order = SmallOrder::default();

    for tag in tags {
        let t = tag.to_vec(); // Vec<String>
        if t.is_empty() {
            continue;
        }

        let key = t[0].as_str();
        let values = &t[1..];

        let v = values.first().map(|s| s.as_str()).unwrap_or_default();

        match key {
            "d" => {
                order.id = Uuid::parse_str(v).ok();
            }
            "k" => {
                order.kind = Kind::from_str(v).ok();
            }
            "f" => {
                order.fiat_code = v.to_string();
            }
            "s" => {
                order.status = Status::from_str(v).ok().or(Some(Status::Pending));
            }
            "amt" => {
                order.amount = v.parse::<i64>().unwrap_or(0);
            }
            "fa" => {
                if v.contains('.') {
                    continue;
                }
                if let Some(max_str) = values.get(1) {
                    order.min_amount = v.parse::<i64>().ok();
                    order.max_amount = max_str.parse::<i64>().ok();
                } else {
                    order.fiat_amount = v.parse::<i64>().unwrap_or(0);
                }
            }
            "pm" => {
                order.payment_method = values.iter().map(|s| s.to_string()).collect();
            }
            "premium" => {
                order.premium = v.parse::<i64>().unwrap_or(0);
            }
            _ => {}
        }
    }

    Ok(order)
}

pub fn dispute_from_tags(tags: Tags) -> Result<Dispute> {
    let mut dispute = Dispute::default();
    for tag in tags {
        let t = tag.to_vec();
        let v = t.get(1).unwrap().as_str();
        match t.first().unwrap().as_str() {
            "d" => {
                let id = t.get(1).unwrap().as_str().parse::<Uuid>();
                let id = match id {
                    core::result::Result::Ok(id) => id,
                    Err(_) => return Err(anyhow::anyhow!("Invalid dispute id")),
                };
                dispute.id = id;
            }

            "s" => {
                let status = match DisputeStatus::from_str(v) {
                    core::result::Result::Ok(status) => status,
                    Err(_) => return Err(anyhow::anyhow!("Invalid dispute status")),
                };

                dispute.status = status.to_string();
            }

            _ => {}
        }
    }

    Ok(dispute)
}
