use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

#[derive(Clone, Debug)]
pub enum Event {
    SmallOrder(SmallOrder),
    Dispute(Dispute),
    MessageTuple(Box<(Message, u64, PublicKey)>),
}

#[derive(Clone, Debug)]
pub enum ListKind {
    Orders,
    Disputes,
    DirectMessagesUser,
    DirectMessagesAdmin,
    PrivateDirectMessagesUser,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum MessageType {
    PrivateDirectMessage,
    PrivateGiftWrap,
    SignedGiftWrap,
}
