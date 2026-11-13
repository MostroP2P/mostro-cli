//! `admin.order-info` data: the node's snapshot of one of its orders, for its
//! admin and its solvers.
//!
//! The reply of the node is untrusted input and is mapped field by field: no
//! core object is serialized wholesale. Fields shared with `order.info` keep
//! its names and formats. The node's statuses and states are kept as the text
//! it sent (open enums): the command shows what the node holds, including a
//! value added after this program was built.

use mostro_core::prelude::{BondInfo, CashuEscrowInfo, HoldInvoiceInfo, OrderInfo, PayoutInfo};
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;

use super::info::{time_fields, trade_key};
use super::schema::{self, OutcomeRules};
use super::{ErrorCode, WarningCode, Warnings};

pub const COMMAND: &str = "admin.order-info";

/// Codes that can fail `admin.order-info` once it is resolved: those of a
/// request, with the admin key in place of the local identity.
pub const ERRORS: [ErrorCode; 14] = [
    ErrorCode::InvalidArguments,
    ErrorCode::ConfigurationMissing,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::RelayConnectionFailed,
    ErrorCode::RelayReadFailed,
    ErrorCode::InternalError,
    ErrorCode::AdminKeyMissing,
    ErrorCode::AdminKeyInvalid,
    ErrorCode::ProtocolUnsupported,
    ErrorCode::PowRequired,
    ErrorCode::RequestRejected,
    ErrorCode::ReplyTimeout,
    ErrorCode::NodeRefused,
    ErrorCode::ReplyMalformed,
];

/// Warnings of `admin.order-info` beyond the pilot pair.
pub const WARNINGS: [WarningCode; 1] = [WarningCode::InvalidOrderTimestamp];

/// What an `admin.order-info` outcome may contain.
pub fn rules() -> OutcomeRules<'static> {
    OutcomeRules::request(COMMAND, &ERRORS, &WARNINGS)
}

/// The value used when a wire enum cannot be named, the fallback
/// `mostro-core` itself uses for a value it does not know.
const UNKNOWN: &str = "unknown";

/// The seller's hold invoice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct HoldInvoiceRecord {
    /// Its state at the node's Lightning node, as the node spelled it:
    /// `open`, `accepted`, `settled`, `canceled`, `not-found` (no record of
    /// the hash), `unavailable` (the Lightning node could not be asked).
    pub(crate) state: String,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) held_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) held_at_unix: Option<String>,
}

/// The Cashu escrow of a Cashu order. Never the token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CashuEscrowRecord {
    pub(crate) mint_url: String,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) locked_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) locked_at_unix: Option<String>,
}

/// The payment to the buyer, while one is in flight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PayoutRecord {
    /// Its state at the node's Lightning node, as the node spelled it:
    /// `in-flight`, `succeeded`, `failed`, `not-found`, `unavailable`.
    pub(crate) state: String,
    /// Why it failed, as the node spelled it: `timeout`, `no-route`, `error`,
    /// `incorrect-payment-details`, `insufficient-balance`, `canceled`.
    pub(crate) failure_reason: Option<String>,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) resolved_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) resolved_at_unix: Option<String>,
}

/// One bond row the node reports for the order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct BondRecord {
    /// `maker` or `taker`, as the node spelled it.
    pub(crate) role: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) amount_sats: String,
    /// `requested`, `locked`, `released`, `pending-payout`, `slashed`,
    /// `forfeited` or `failed`, as the node spelled it.
    pub(crate) state: String,
    /// `lost-dispute` or `timeout`, as the node spelled it.
    pub(crate) slash_reason: Option<String>,
    /// `bond` (the bond itself), `slice-slash` (a maker slash on a slice of a
    /// range order) or `maker-refund` (the remainder returned to the maker).
    pub(crate) kind: String,
    /// The root order, when the row is the maker bond of the range order
    /// this slice belongs to.
    #[schemars(schema_with = "schema::nullable_uuid")]
    pub(crate) root_order_id: Option<String>,
}

/// `admin.order-info` success data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AdminOrderInfoData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    pub(crate) node_pubkey: String,
    #[schemars(schema_with = "schema::uuid")]
    pub(crate) order_id: String,
    /// `buy` or `sell`, as the node spelled it.
    pub(crate) side: String,
    /// The status stored in the node's database, in its spelling: not the
    /// reduced status of the public order event.
    pub(crate) status: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) sats_amount: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) fee_sats: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) routing_fee_sats: String,
    pub(crate) fiat_currency: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) fiat_amount: String,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) fiat_amount_min: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) fiat_amount_max: Option<String>,
    pub(crate) payment_method: String,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) premium_percent: String,
    #[schemars(schema_with = "schema::nullable_uuid")]
    pub(crate) range_parent_id: Option<String>,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) created_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) created_at_unix: Option<String>,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) taken_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) taken_at_unix: Option<String>,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) expires_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) expires_at_unix: Option<String>,
    #[schemars(schema_with = "schema::nullable_pubkey")]
    pub(crate) buyer_trade_pubkey: Option<String>,
    #[schemars(schema_with = "schema::nullable_pubkey")]
    pub(crate) seller_trade_pubkey: Option<String>,
    pub(crate) buyer_dispute: bool,
    pub(crate) seller_dispute: bool,
    pub(crate) buyer_cooperative_cancel: bool,
    pub(crate) seller_cooperative_cancel: bool,
    /// The node's `failed_payment` flag for the payment to the buyer.
    pub(crate) failed_payment: bool,
    #[schemars(schema_with = "schema::decimal")]
    pub(crate) payment_attempts: String,
    #[schemars(schema_with = "schema::nullable_rfc3339")]
    pub(crate) hold_settled_at: Option<String>,
    #[schemars(schema_with = "schema::nullable_decimal")]
    pub(crate) hold_settled_at_unix: Option<String>,
    /// `null` when the order has no hold invoice hash.
    pub(crate) hold_invoice: Option<HoldInvoiceRecord>,
    /// `null` unless the order is a Cashu order.
    pub(crate) cashu_escrow: Option<CashuEscrowRecord>,
    /// `null` unless a payment to the buyer is in flight.
    pub(crate) payout: Option<PayoutRecord>,
    /// Oldest first; empty when the node reports no bond for the order.
    pub(crate) bonds: Vec<BondRecord>,
}

impl AdminOrderInfoData {
    /// Map the node's `reply`. One warning marks times that cannot be
    /// represented, which are `null` in their text form.
    pub fn from_reply(node: &PublicKey, reply: &OrderInfo) -> (Self, Warnings) {
        let mut warnings = Warnings::new();
        let (created_at, created_at_unix) = time_fields(Some(reply.created_at), &mut warnings);
        let (taken_at, taken_at_unix) = time_fields(reply.taken_at, &mut warnings);
        let (expires_at, expires_at_unix) = time_fields(Some(reply.expires_at), &mut warnings);
        let (hold_settled_at, hold_settled_at_unix) =
            time_fields(reply.hold_settled_at, &mut warnings);
        let data = Self {
            node_pubkey: node.to_hex(),
            order_id: reply.id.to_string(),
            side: reply.kind.clone(),
            status: reply.status.clone(),
            sats_amount: reply.amount.to_string(),
            fee_sats: reply.fee.to_string(),
            routing_fee_sats: reply.routing_fee.to_string(),
            fiat_currency: reply.fiat_code.clone(),
            fiat_amount: reply.fiat_amount.to_string(),
            fiat_amount_min: reply.min_amount.map(|amount| amount.to_string()),
            fiat_amount_max: reply.max_amount.map(|amount| amount.to_string()),
            payment_method: reply.payment_method.clone(),
            premium_percent: reply.premium.to_string(),
            range_parent_id: reply.range_parent_id.map(|id| id.to_string()),
            created_at,
            created_at_unix,
            taken_at,
            taken_at_unix,
            expires_at,
            expires_at_unix,
            buyer_trade_pubkey: trade_key(reply.buyer_trade_pubkey.as_deref()),
            seller_trade_pubkey: trade_key(reply.seller_trade_pubkey.as_deref()),
            buyer_dispute: reply.buyer_dispute,
            seller_dispute: reply.seller_dispute,
            buyer_cooperative_cancel: reply.buyer_cooperativecancel,
            seller_cooperative_cancel: reply.seller_cooperativecancel,
            failed_payment: reply.failed_payment,
            payment_attempts: reply.payment_attempts.to_string(),
            hold_settled_at,
            hold_settled_at_unix,
            hold_invoice: reply
                .hold_invoice
                .as_ref()
                .map(|hold| hold_invoice(hold, &mut warnings)),
            cashu_escrow: reply
                .cashu_escrow
                .as_ref()
                .map(|escrow| cashu_escrow(escrow, &mut warnings)),
            payout: reply
                .payout
                .as_ref()
                .map(|payout| payout_record(payout, &mut warnings)),
            bonds: reply.bonds.iter().map(bond).collect(),
        };
        (data, warnings)
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn status(&self) -> &str {
        &self.status
    }
}

/// The name a `mostro-core` wire enum travels with, so this program keeps no
/// list of its own.
fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| UNKNOWN.to_string())
}

fn hold_invoice(hold: &HoldInvoiceInfo, warnings: &mut Warnings) -> HoldInvoiceRecord {
    let (held_at, held_at_unix) = time_fields(hold.held_at, warnings);
    HoldInvoiceRecord {
        state: wire(&hold.state),
        held_at,
        held_at_unix,
    }
}

fn cashu_escrow(escrow: &CashuEscrowInfo, warnings: &mut Warnings) -> CashuEscrowRecord {
    let (locked_at, locked_at_unix) = time_fields(escrow.locked_at, warnings);
    CashuEscrowRecord {
        mint_url: escrow.mint_url.clone(),
        locked_at,
        locked_at_unix,
    }
}

fn payout_record(payout: &PayoutInfo, warnings: &mut Warnings) -> PayoutRecord {
    let (resolved_at, resolved_at_unix) = time_fields(payout.resolved_at, warnings);
    PayoutRecord {
        state: wire(&payout.state),
        failure_reason: payout.failure_reason.as_ref().map(wire),
        resolved_at,
        resolved_at_unix,
    }
}

fn bond(bond: &BondInfo) -> BondRecord {
    BondRecord {
        role: wire(&bond.role),
        amount_sats: bond.amount.to_string(),
        state: wire(&bond.state),
        slash_reason: bond.slash_reason.as_ref().map(wire),
        kind: wire(&bond.kind),
        root_order_id: bond.root_order_id.map(|id| id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::prelude::{
        BondKind, BondRole, BondSlashReason, BondState, HoldInvoiceState, PayoutFailureReason,
        PayoutState,
    };
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, Value};
    use uuid::Uuid;

    const ORDER: Uuid = Uuid::from_u128(1);
    const ROOT: Uuid = Uuid::from_u128(2);

    /// A Lightning order with every part answered.
    fn full() -> OrderInfo {
        OrderInfo {
            id: ORDER,
            kind: "sell".to_string(),
            status: "settled-hold-invoice".to_string(),
            amount: 100_000,
            fee: 600,
            routing_fee: 12,
            fiat_code: "EUR".to_string(),
            fiat_amount: 50,
            min_amount: Some(10),
            max_amount: Some(100),
            payment_method: "SEPA".to_string(),
            premium: -3,
            range_parent_id: Some(ROOT),
            created_at: 1_792_108_800,
            taken_at: Some(1_792_108_900),
            expires_at: 1_792_195_200,
            buyer_trade_pubkey: Some(Keys::generate().public_key().to_hex()),
            seller_trade_pubkey: None,
            buyer_dispute: false,
            seller_dispute: true,
            buyer_cooperativecancel: true,
            seller_cooperativecancel: false,
            failed_payment: true,
            payment_attempts: 2,
            hold_settled_at: Some(1_792_109_050),
            hold_invoice: Some(HoldInvoiceInfo {
                state: HoldInvoiceState::Settled,
                held_at: Some(1_792_109_000),
            }),
            cashu_escrow: None,
            payout: Some(PayoutInfo {
                state: PayoutState::Failed,
                failure_reason: Some(PayoutFailureReason::NoRoute),
                resolved_at: Some(1_792_109_100),
            }),
            bonds: vec![
                BondInfo {
                    role: BondRole::Maker,
                    amount: 5_000,
                    state: BondState::Locked,
                    slash_reason: None,
                    kind: BondKind::Bond,
                    root_order_id: Some(ROOT),
                },
                BondInfo {
                    role: BondRole::Maker,
                    amount: 2_000,
                    state: BondState::PendingPayout,
                    slash_reason: Some(BondSlashReason::LostDispute),
                    kind: BondKind::SliceSlash,
                    root_order_id: None,
                },
            ],
        }
    }

    fn mapped(reply: &OrderInfo) -> (Value, Warnings) {
        let node = Keys::generate().public_key();
        let (data, warnings) = AdminOrderInfoData::from_reply(&node, reply);
        assert_eq!(data.node_pubkey, node.to_hex());
        (serde_json::to_value(data).unwrap(), warnings)
    }

    #[test]
    fn every_field_of_the_reply_is_mapped_in_the_formats_of_order_info() {
        let reply = full();
        let (data, warnings) = mapped(&reply);

        assert!(warnings.is_empty());
        assert_eq!(data["order_id"], ORDER.to_string());
        assert_eq!(data["side"], "sell");
        assert_eq!(data["status"], "settled-hold-invoice");
        // Amounts and counts are decimal strings.
        assert_eq!(data["sats_amount"], "100000");
        assert_eq!(data["fee_sats"], "600");
        assert_eq!(data["routing_fee_sats"], "12");
        assert_eq!(data["fiat_currency"], "EUR");
        assert_eq!(data["fiat_amount"], "50");
        assert_eq!(data["fiat_amount_min"], "10");
        assert_eq!(data["fiat_amount_max"], "100");
        assert_eq!(data["payment_method"], "SEPA");
        assert_eq!(data["premium_percent"], "-3");
        assert_eq!(data["range_parent_id"], ROOT.to_string());
        // Each time in both forms.
        assert_eq!(data["created_at"], "2026-10-16T00:00:00Z");
        assert_eq!(data["created_at_unix"], "1792108800");
        assert_eq!(data["taken_at"], "2026-10-16T00:01:40Z");
        assert_eq!(data["taken_at_unix"], "1792108900");
        assert_eq!(data["expires_at"], "2026-10-17T00:00:00Z");
        assert_eq!(data["expires_at_unix"], "1792195200");
        assert_eq!(data["buyer_trade_pubkey"], json!(reply.buyer_trade_pubkey));
        assert_eq!(data["seller_trade_pubkey"], Value::Null);
        assert_eq!(data["buyer_dispute"], false);
        assert_eq!(data["seller_dispute"], true);
        assert_eq!(data["buyer_cooperative_cancel"], true);
        assert_eq!(data["seller_cooperative_cancel"], false);
        assert_eq!(data["failed_payment"], true);
        assert_eq!(data["payment_attempts"], "2");
        assert_eq!(data["hold_settled_at"], "2026-10-16T00:04:10Z");
        assert_eq!(data["hold_settled_at_unix"], "1792109050");
        assert_eq!(
            data["hold_invoice"],
            json!({"state": "settled", "held_at": "2026-10-16T00:03:20Z",
                   "held_at_unix": "1792109000"})
        );
        assert_eq!(data["cashu_escrow"], Value::Null);
        assert_eq!(
            data["payout"],
            json!({"state": "failed", "failure_reason": "no-route",
                   "resolved_at": "2026-10-16T00:05:00Z", "resolved_at_unix": "1792109100"})
        );
        assert_eq!(
            data["bonds"],
            json!([
                {"role": "maker", "amount_sats": "5000", "state": "locked", "slash_reason": null,
                 "kind": "bond", "root_order_id": ROOT.to_string()},
                {"role": "maker", "amount_sats": "2000", "state": "pending-payout",
                 "slash_reason": "lost-dispute", "kind": "slice-slash", "root_order_id": null},
            ])
        );
    }

    #[test]
    fn a_pending_order_has_no_lightning_part_and_no_time_it_does_not_hold() {
        let (data, warnings) = mapped(&OrderInfo {
            status: "pending".to_string(),
            taken_at: None,
            hold_settled_at: None,
            hold_invoice: None,
            payout: None,
            bonds: Vec::new(),
            ..full()
        });

        assert!(warnings.is_empty());
        for absent in [
            "taken_at",
            "taken_at_unix",
            "hold_settled_at",
            "hold_settled_at_unix",
            "hold_invoice",
            "cashu_escrow",
            "payout",
        ] {
            assert_eq!(data[absent], Value::Null, "{absent}");
        }
        assert_eq!(data["bonds"], json!([]));
    }

    #[test]
    fn a_cashu_order_carries_its_escrow() {
        let (data, _) = mapped(&OrderInfo {
            hold_invoice: None,
            payout: None,
            cashu_escrow: Some(CashuEscrowInfo {
                mint_url: "https://mint.example".to_string(),
                locked_at: None,
            }),
            ..full()
        });

        assert_eq!(
            data["cashu_escrow"],
            json!({"mint_url": "https://mint.example", "locked_at": null, "locked_at_unix": null})
        );
        assert_eq!(data["hold_invoice"], Value::Null);
    }

    /// The states that are not the Lightning node's own answer travel like
    /// any other.
    #[test]
    fn not_found_and_unavailable_are_states() {
        for (state, name) in [
            (HoldInvoiceState::NotFound, "not-found"),
            (HoldInvoiceState::Unavailable, "unavailable"),
        ] {
            let (data, _) = mapped(&OrderInfo {
                hold_invoice: Some(HoldInvoiceInfo {
                    state,
                    held_at: None,
                }),
                ..full()
            });
            assert_eq!(data["hold_invoice"]["state"], name);
            assert_eq!(data["status"], "settled-hold-invoice");
        }
    }

    /// SC-004: a node newer than this program. Its status is kept as text,
    /// and a state `mostro-core` cannot name is `unknown`, never an error.
    #[test]
    fn values_added_by_a_later_node_are_not_an_error() {
        let mut reply = serde_json::to_value(full()).unwrap();
        reply["status"] = json!("a-status-added-later");
        reply["kind"] = json!("swap");
        reply["hold_invoice"]["state"] = json!("a-state-added-later");
        reply["payout"]["state"] = json!("a-state-added-later");
        reply["payout"]["failure_reason"] = json!("a-reason-added-later");
        reply["bonds"][0]["state"] = json!("a-state-added-later");
        reply["bonds"][0]["kind"] = json!("a-kind-added-later");
        reply["added_by_a_later_node"] = json!({"any": "thing"});
        let reply: OrderInfo = serde_json::from_value(reply).unwrap();

        let (data, warnings) = mapped(&reply);

        assert!(warnings.is_empty());
        assert_eq!(data["status"], "a-status-added-later");
        assert_eq!(data["side"], "swap");
        assert_eq!(data["hold_invoice"]["state"], "unknown");
        assert_eq!(data["payout"]["state"], "unknown");
        assert_eq!(data["payout"]["failure_reason"], "unknown");
        assert_eq!(data["bonds"][0]["state"], "unknown");
        assert_eq!(data["bonds"][0]["kind"], "unknown");
    }

    #[test]
    fn a_time_that_cannot_be_shown_is_null_with_one_warning() {
        let (data, warnings) = mapped(&OrderInfo {
            created_at: i64::MAX,
            taken_at: Some(i64::MAX),
            ..full()
        });

        assert_eq!(data["created_at"], Value::Null);
        assert_eq!(data["created_at_unix"], i64::MAX.to_string());
        assert_eq!(data["taken_at"], Value::Null);
        assert!(warnings.contains(WarningCode::InvalidOrderTimestamp));
        assert_eq!(
            serde_json::to_value(&warnings)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn a_trade_key_that_is_not_a_key_is_dropped() {
        let (data, _) = mapped(&OrderInfo {
            buyer_trade_pubkey: Some("not a key".to_string()),
            ..full()
        });
        assert_eq!(data["buyer_trade_pubkey"], Value::Null);
    }

    fn keys_of(value: &Value, keys: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map {
                    keys.push(key.clone());
                    keys_of(inner, keys);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| keys_of(item, keys)),
            _ => {}
        }
    }

    #[test]
    fn the_data_has_no_field_for_a_secret() {
        let (data, _) = mapped(&OrderInfo {
            cashu_escrow: Some(CashuEscrowInfo {
                mint_url: "https://mint.example".to_string(),
                locked_at: None,
            }),
            ..full()
        });
        let mut keys = Vec::new();
        keys_of(&data, &mut keys);
        for key in keys.iter().filter(|key| *key != "hold_invoice") {
            for forbidden in ["preimage", "invoice", "token", "master", "hash", "nsec"] {
                assert!(!key.contains(forbidden), "`{key}`");
            }
        }
    }

    #[test]
    fn the_rules_are_those_of_a_request_with_the_admin_key_codes() {
        let rules = rules();
        assert_eq!(rules.command, "admin.order-info");
        assert!(rules.reason, "a refusal carries the node's reason");
        assert!(rules.uncertain.is_empty(), "nothing is written");
        for code in [ErrorCode::AdminKeyMissing, ErrorCode::AdminKeyInvalid] {
            assert!(rules.errors.contains(&code));
        }
        for code in [ErrorCode::IdentityMissing, ErrorCode::IdentityUnreadable] {
            assert!(!rules.errors.contains(&code), "no local identity is read");
        }
    }
}
