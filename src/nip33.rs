use anyhow::{Ok, Result};
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

pub fn order_from_tags(tags: Vec<Tag>) -> Result<SmallOrder> {
    let mut order = SmallOrder::default();
    for tag in tags {
        let t = tag.as_vec();
        let v = t.get(1).unwrap().as_str();
        match t.get(0).unwrap().as_str() {
            "d" => {
                let id = t.get(1).unwrap().as_str().parse::<Uuid>();
                let id = match id {
                    core::result::Result::Ok(id) => Some(id),
                    Err(_) => None,
                };
                order.id = id;
            }
            "k" => {
                order.kind = Some(OrderKind::from_str(v).unwrap());
            }
            "f" => {
                order.fiat_code = v.to_string();
            }
            "s" => {
                order.status = Some(Status::from_str(v).unwrap());
            }
            "amt" => {
                order.amount = v.parse::<i64>().unwrap();
            }
            "fa" => {
                order.fiat_amount = v.parse::<i64>().unwrap();
            }
            "pm" => {
                order.payment_method = v.to_string();
            }
            "premium" => {
                order.premium = v.parse::<i64>().unwrap();
            }
            _ => {}
        }
    }

    Ok(order)
}
