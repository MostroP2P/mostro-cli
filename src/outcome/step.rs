//! `order.fiat-sent`, `order.release` and `order.cancel` data: the step the
//! user took on an order they already hold and what the node answered.
//!
//! The node stays the authority (who may take the step, in which status). The
//! reply is untrusted: it must name the order and map to an explicit result.
//! Nothing the user typed beyond the order id appears in the data.

use mostro_core::order::Status;
use mostro_core::prelude::{Action, Payload};
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::{schema, ErrorCode, WarningCode};

/// Codes that fail a step with a plain error (nothing sent, or a refusal).
pub const STEP_ERRORS: [ErrorCode; 15] = [
    ErrorCode::InvalidArguments,
    ErrorCode::ConfigurationMissing,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::RelayConnectionFailed,
    ErrorCode::RelayReadFailed,
    ErrorCode::InternalError,
    ErrorCode::IdentityMissing,
    ErrorCode::IdentityUnreadable,
    ErrorCode::ProtocolUnsupported,
    ErrorCode::PowRequired,
    ErrorCode::RequestRejected,
    ErrorCode::NodeRefused,
    ErrorCode::OrderDeclined,
    ErrorCode::StoreUnwritable,
    ErrorCode::OrderUnknown,
];

/// Codes that leave a step `uncertain`: the node may have acted.
pub const STEP_UNCERTAIN: [ErrorCode; 5] = [
    ErrorCode::ReplyTimeout,
    ErrorCode::ReplyMalformed,
    ErrorCode::RequestRejected,
    ErrorCode::RelayReadFailed,
    ErrorCode::PowRequired,
];

/// Warnings of the steps beyond the pilot pair.
pub const STEP_WARNINGS: [WarningCode; 2] = [
    WarningCode::LocalStatusStale,
    WarningCode::TradeIndexNotRecorded,
];

/// Which step is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    /// The buyer says the fiat was sent.
    FiatSent,
    /// The seller settles the hold invoice.
    Release,
    /// The maker or the taker cancels; the node decides the kind of cancel.
    Cancel,
}

impl StepKind {
    pub fn command(self) -> &'static str {
        match self {
            Self::FiatSent => "order.fiat-sent",
            Self::Release => "order.release",
            Self::Cancel => "order.cancel",
        }
    }

    pub fn action(self) -> Action {
        match self {
            Self::FiatSent => Action::FiatSent,
            Self::Release => Action::Release,
            Self::Cancel => Action::Cancel,
        }
    }

    /// Whether the request carries a next trade key when the order is a range
    /// order of the user's with room left: the maker's own step. A seller
    /// (`sells`) sends it with `release`, a buyer with `fiat-sent`, where the
    /// node keeps it until the peer releases.
    pub fn sends_key(self, sells: bool) -> bool {
        match self {
            Self::Release => sells,
            Self::FiatSent => !sells,
            Self::Cancel => false,
        }
    }

    /// The actions the node answers this step with, besides `cant-do`.
    pub fn replies(self) -> Vec<Action> {
        match self {
            Self::FiatSent => vec![Action::FiatSentOk],
            Self::Release => vec![Action::HoldInvoicePaymentSettled, Action::Released],
            Self::Cancel => vec![
                Action::Canceled,
                Action::CooperativeCancelInitiatedByYou,
                Action::CooperativeCancelAccepted,
            ],
        }
    }
}

/// A step on an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepInput {
    kind: StepKind,
    order: Uuid,
}

impl StepInput {
    pub fn new(kind: StepKind, order: Uuid) -> Self {
        Self { kind, order }
    }

    pub fn kind(&self) -> StepKind {
        self.kind
    }

    pub fn order(&self) -> Uuid {
        self.order
    }
}

/// What the node did with the step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepResult {
    /// The order is now `fiat-sent`.
    FiatSent,
    /// The hold invoice was settled; the buyer is paid.
    Released,
    /// The order is canceled.
    Canceled,
    /// A cooperative cancel was started; the other party has to confirm.
    CancelRequested,
    /// The cooperative cancel was confirmed: the order is canceled by both.
    CancelAccepted,
}

/// What the user can usefully do next, only where a command or a wait is
/// meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepNext {
    /// Wait for the seller to release the sats.
    WaitForRelease,
    /// Wait for the other party to confirm with `order cancel`.
    WaitForPeerCancel,
    /// The node created a child order for the rest of the range; `order sync`
    /// gives it a local row.
    SyncChildOrder,
}

/// A step the node answered, as the data of the reply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct StepData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::uuid")]
    order_id: String,
    result: StepResult,
    next_step: Option<StepNext>,
    /// The public key of the order's trade key.
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
    /// The trade index reserved for the next order of a range; null when the
    /// request carried none.
    #[schemars(schema_with = "schema::nullable_decimal")]
    next_trade_index: Option<String>,
}

impl StepData {
    /// Map the node's reply to `input`: the data and the status to record
    /// locally (`None` when the reply gives no new one). `None` when the reply
    /// is not one this step may be answered with, names another order or
    /// carries an unexpected payload.
    pub fn from_reply(
        node: &PublicKey,
        input: &StepInput,
        reply: (&Action, &Option<Payload>, Option<Uuid>),
        trade_pubkey: &PublicKey,
        next_trade_index: Option<i64>,
    ) -> Option<(Self, Option<Status>)> {
        let (action, payload, id) = reply;
        if id != Some(input.order) || !input.kind.replies().contains(action) {
            return None;
        }
        let bare = payload.is_none();
        let (result, next_step, status) = match (action, input.kind) {
            (Action::FiatSentOk, _) if matches!(payload, None | Some(Payload::Peer(_))) => (
                StepResult::FiatSent,
                Some(StepNext::WaitForRelease),
                Some(Status::FiatSent),
            ),
            (Action::HoldInvoicePaymentSettled | Action::Released, _) if bare => (
                StepResult::Released,
                next_trade_index.map(|_| StepNext::SyncChildOrder),
                Some(Status::SettledHoldInvoice),
            ),
            (Action::Canceled, _) if bare => (StepResult::Canceled, None, Some(Status::Canceled)),
            (Action::CooperativeCancelInitiatedByYou, _) if bare => (
                StepResult::CancelRequested,
                Some(StepNext::WaitForPeerCancel),
                None,
            ),
            (Action::CooperativeCancelAccepted, _) if bare => (
                StepResult::CancelAccepted,
                None,
                Some(Status::CooperativelyCanceled),
            ),
            _ => return None,
        };
        let data = Self {
            node_pubkey: node.to_hex(),
            order_id: input.order.to_string(),
            result,
            next_step,
            trade_pubkey: trade_pubkey.to_hex(),
            next_trade_index: next_trade_index.map(|index| index.to_string()),
        };
        Some((data, status))
    }

    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    pub fn result(&self) -> StepResult {
        self.result
    }

    pub fn next_step(&self) -> Option<StepNext> {
        self.next_step
    }

    pub fn trade_pubkey(&self) -> &str {
        &self.trade_pubkey
    }

    pub fn node_pubkey(&self) -> &str {
        &self.node_pubkey
    }

    pub fn next_trade_index(&self) -> Option<&str> {
        self.next_trade_index.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::schema::schema_check::conforms;
    use crate::outcome::schema::{output_schema, OutcomeRules};
    use crate::outcome::{
        DaemonAcceptance, Envelope, ExecutionMeta, Persistence, Publication, Warnings,
    };
    use mostro_core::prelude::Peer;
    use nostr_sdk::prelude::Keys;
    use serde_json::{json, Value};

    fn order_id() -> Uuid {
        Uuid::from_u128(9)
    }

    fn keys() -> (PublicKey, PublicKey) {
        (Keys::generate().public_key(), Keys::generate().public_key())
    }

    fn map(
        kind: StepKind,
        action: Action,
        payload: Option<Payload>,
        id: Option<Uuid>,
        index: Option<i64>,
    ) -> Option<(StepData, Option<Status>)> {
        let (node, trade) = keys();
        StepData::from_reply(
            &node,
            &StepInput::new(kind, order_id()),
            (&action, &payload, id),
            &trade,
            index,
        )
    }

    fn peer() -> Option<Payload> {
        Some(Payload::Peer(Peer {
            pubkey: Keys::generate().public_key().to_string(),
            reputation: None,
        }))
    }

    #[test]
    fn the_kinds_name_their_command_action_and_replies() {
        let table = [
            (StepKind::FiatSent, "order.fiat-sent", Action::FiatSent),
            (StepKind::Release, "order.release", Action::Release),
            (StepKind::Cancel, "order.cancel", Action::Cancel),
        ];
        for (kind, command, action) in table {
            assert_eq!((kind.command(), kind.action()), (command, action));
            assert!(!kind.replies().is_empty());
            assert!(!kind.replies().contains(&Action::CantDo));
        }
        assert!(StepKind::Release.sends_key(true) && !StepKind::Release.sends_key(false));
        assert!(StepKind::FiatSent.sends_key(false) && !StepKind::FiatSent.sends_key(true));
        assert!(!StepKind::Cancel.sends_key(true) && !StepKind::Cancel.sends_key(false));
    }

    #[test]
    fn each_reply_maps_to_its_result_next_step_and_status_to_record() {
        let id = Some(order_id());
        let cases = [
            (
                StepKind::FiatSent,
                Action::FiatSentOk,
                peer(),
                StepResult::FiatSent,
                Some(StepNext::WaitForRelease),
                Some(Status::FiatSent),
            ),
            (
                StepKind::FiatSent,
                Action::FiatSentOk,
                None,
                StepResult::FiatSent,
                Some(StepNext::WaitForRelease),
                Some(Status::FiatSent),
            ),
            (
                StepKind::Release,
                Action::HoldInvoicePaymentSettled,
                None,
                StepResult::Released,
                None,
                Some(Status::SettledHoldInvoice),
            ),
            (
                StepKind::Release,
                Action::Released,
                None,
                StepResult::Released,
                None,
                Some(Status::SettledHoldInvoice),
            ),
            (
                StepKind::Cancel,
                Action::Canceled,
                None,
                StepResult::Canceled,
                None,
                Some(Status::Canceled),
            ),
            (
                StepKind::Cancel,
                Action::CooperativeCancelInitiatedByYou,
                None,
                StepResult::CancelRequested,
                Some(StepNext::WaitForPeerCancel),
                None,
            ),
            (
                StepKind::Cancel,
                Action::CooperativeCancelAccepted,
                None,
                StepResult::CancelAccepted,
                None,
                Some(Status::CooperativelyCanceled),
            ),
        ];
        for (kind, action, payload, result, next, status) in cases {
            let label = format!("{kind:?} {action:?}");
            let (data, recorded) = map(kind, action, payload, id, None).expect(&label);
            assert_eq!(data.result(), result, "{label}");
            assert_eq!(data.next_step(), next, "{label}");
            assert_eq!(recorded, status, "{label}");
            assert_eq!(data.order_id(), order_id().to_string(), "{label}");
            assert_eq!(data.next_trade_index(), None, "{label}");
        }
    }

    #[test]
    fn the_reserved_index_is_reported_as_a_decimal_string() {
        let (data, _) = map(
            StepKind::Release,
            Action::HoldInvoicePaymentSettled,
            None,
            Some(order_id()),
            Some(12),
        )
        .unwrap();
        assert_eq!(data.next_trade_index(), Some("12"));
        assert_eq!(
            data.next_step(),
            Some(StepNext::SyncChildOrder),
            "a release that sent a key points at order sync"
        );
    }

    #[test]
    fn a_fiat_sent_that_sent_a_key_still_waits_for_the_release() {
        let (data, _) = map(
            StepKind::FiatSent,
            Action::FiatSentOk,
            None,
            Some(order_id()),
            Some(12),
        )
        .unwrap();
        assert_eq!(data.next_trade_index(), Some("12"));
        assert_eq!(data.next_step(), Some(StepNext::WaitForRelease));
    }

    #[test]
    fn a_reply_that_does_not_answer_the_step_cannot_be_used() {
        let id = Some(order_id());
        // Another step's reply, another order, no id, an unexpected payload.
        assert!(map(StepKind::FiatSent, Action::Canceled, None, id, None).is_none());
        assert!(map(StepKind::Release, Action::FiatSentOk, None, id, None).is_none());
        assert!(map(
            StepKind::Cancel,
            Action::HoldInvoicePaymentSettled,
            None,
            id,
            None
        )
        .is_none());
        assert!(map(StepKind::Cancel, Action::CantDo, None, id, None).is_none());
        let other = Some(Uuid::from_u128(10));
        assert!(map(StepKind::Cancel, Action::Canceled, None, other, None).is_none());
        assert!(map(StepKind::Cancel, Action::Canceled, None, None, None).is_none());
        assert!(map(StepKind::Cancel, Action::Canceled, peer(), id, None).is_none());
        assert!(map(
            StepKind::FiatSent,
            Action::FiatSentOk,
            Some(Payload::Amount(1)),
            id,
            None
        )
        .is_none());
    }

    fn schema_of(kind: StepKind) -> Value {
        Value::Object(output_schema::<StepData, ()>(&OutcomeRules::step(
            kind.command(),
            &STEP_ERRORS,
            &STEP_UNCERTAIN,
            &STEP_WARNINGS,
        )))
    }

    fn meta(
        publication: Publication,
        acceptance: DaemonAcceptance,
        p: Persistence,
    ) -> ExecutionMeta<()> {
        ExecutionMeta::write(publication, acceptance, p)
    }

    fn ok_outcome(kind: StepKind, persistence: Persistence, index: Option<i64>) -> Value {
        let (action, payload) = match kind {
            StepKind::FiatSent => (Action::FiatSentOk, peer()),
            StepKind::Release => (Action::HoldInvoicePaymentSettled, None),
            StepKind::Cancel => (Action::Canceled, None),
        };
        let (data, _) = map(kind, action, payload, Some(order_id()), index).unwrap();
        serde_json::to_value(Envelope::ok(
            kind.command(),
            data,
            Warnings::new(),
            meta(
                Publication::Published,
                DaemonAcceptance::Accepted,
                persistence,
            ),
        ))
        .unwrap()
    }

    const KINDS: [StepKind; 3] = [StepKind::FiatSent, StepKind::Release, StepKind::Cancel];

    #[test]
    fn real_outcomes_conform_for_every_step_and_persistence() {
        for kind in KINDS {
            let schema = schema_of(kind);
            assert_eq!(schema["title"], format!("{} outcome", kind.command()));
            for persistence in [
                Persistence::Applied,
                Persistence::Failed,
                Persistence::NotAttempted,
            ] {
                for index in [None, Some(4)] {
                    let outcome = ok_outcome(kind, persistence, index);
                    assert!(conforms(&schema, &schema, &outcome), "{outcome}");
                }
            }
        }
    }

    #[test]
    fn a_stale_local_status_warns_and_conforms() {
        let schema = schema_of(StepKind::Release);
        let (data, _) = map(
            StepKind::Release,
            Action::HoldInvoicePaymentSettled,
            None,
            Some(order_id()),
            None,
        )
        .unwrap();
        let mut warnings = Warnings::new();
        warnings.add(WarningCode::LocalStatusStale);
        let stale = serde_json::to_value(Envelope::ok(
            "order.release",
            data,
            warnings,
            meta(
                Publication::Published,
                DaemonAcceptance::Accepted,
                Persistence::Failed,
            ),
        ))
        .unwrap();
        assert!(conforms(&schema, &schema, &stale), "{stale}");
    }

    #[test]
    fn uncertain_and_error_outcomes_conform_with_or_without_a_recorded_index() {
        for kind in KINDS {
            let (schema, command) = (schema_of(kind), kind.command());
            for code in STEP_UNCERTAIN {
                for persistence in [
                    Persistence::NotAttempted,
                    Persistence::Applied,
                    Persistence::Failed,
                ] {
                    let outcome = serde_json::to_value(Envelope::<Value, ()>::uncertain(
                        command,
                        code,
                        None,
                        Warnings::new(),
                        meta(
                            Publication::Published,
                            DaemonAcceptance::Unknown,
                            persistence,
                        ),
                    ))
                    .unwrap();
                    assert!(conforms(&schema, &schema, &outcome), "{command} {code:?}");
                }
            }
            let refused = serde_json::to_value(Envelope::<Value, ()>::refused(
                Some(command),
                "not_allowed_by_status",
                Warnings::new(),
                meta(
                    Publication::Published,
                    DaemonAcceptance::Rejected,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap();
            assert!(conforms(&schema, &schema, &refused), "{command}");
            let unknown = serde_json::to_value(Envelope::<Value, ()>::error(
                Some(command),
                ErrorCode::OrderUnknown,
                Warnings::new(),
                meta(
                    Publication::NotAttempted,
                    DaemonAcceptance::Unknown,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap();
            assert!(conforms(&schema, &schema, &unknown), "{command}");
        }
    }

    #[test]
    fn contradictory_outcomes_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let edits: Vec<(&str, Edit)> = vec![
            (
                "an unknown result",
                Box::new(|o| o["data"]["result"] = json!("paid")),
            ),
            (
                "an unknown next step",
                Box::new(|o| o["data"]["next_step"] = json!("rate")),
            ),
            (
                "a bad order id",
                Box::new(|o| o["data"]["order_id"] = json!("not-a-uuid")),
            ),
            (
                "an uppercase trade key",
                Box::new(|o| o["data"]["trade_pubkey"] = json!("ABC")),
            ),
            (
                "a numeric index",
                Box::new(|o| o["data"]["next_trade_index"] = json!(3)),
            ),
            (
                "a missing index key",
                Box::new(|o| {
                    o["data"]
                        .as_object_mut()
                        .unwrap()
                        .remove("next_trade_index");
                }),
            ),
            (
                "another command",
                Box::new(|o| o["command"] = json!("order.cancel")),
            ),
            (
                "a rejected success",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("rejected")),
            ),
        ];
        let schema = schema_of(StepKind::Release);
        for (label, edit) in edits {
            let mut outcome = ok_outcome(StepKind::Release, Persistence::Applied, None);
            edit(&mut outcome);
            assert!(!conforms(&schema, &schema, &outcome), "{label}");
        }
    }

    #[test]
    fn the_data_has_exactly_the_documented_fields() {
        let outcome = ok_outcome(StepKind::FiatSent, Persistence::Applied, None);
        let data = &outcome["data"];
        let mut fields: Vec<&str> = data
            .as_object()
            .map(|fields| fields.keys().map(String::as_str).collect())
            .unwrap_or_default();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "next_step",
                "next_trade_index",
                "node_pubkey",
                "order_id",
                "result",
                "trade_pubkey"
            ],
            "{data}"
        );
    }
}
