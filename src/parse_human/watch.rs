//! Human presentation of `order.watch`: each update as it comes, then one
//! line that says why the watch stopped. Built from the typed outcome only.
//! Node messages render as `message list` renders them, chat messages as
//! `order chat` does, chat text escaped and indented.

use std::fmt::Write;

use chrono::DateTime;

use super::{chat, message};
use crate::outcome::watch::WatchData;

const RULE: &str = "═══════════════════════════════════════";

/// Where an update sits in a human watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// The first update, of a watch whose window starts at `since` (Unix
    /// seconds).
    First { since: u64 },
    /// A later update; `learned` when it is the first to name the
    /// counterparty.
    Later { learned: bool },
}

/// One update of a human watch: the header first, then the node's messages
/// and the chat messages, each oldest first.
pub fn render_watch_update(data: &WatchData, shown: Shown) -> String {
    let mut out = String::new();
    match (shown, data.counterparty_pubkey()) {
        (Shown::First { since }, peer) => header(&mut out, data.order_id(), since, peer),
        (Shown::Later { learned: true }, Some(peer)) => {
            let _ = writeln!(out, "👤 Counterparty: {peer}; following the chat");
        }
        (Shown::Later { .. }, _) => {}
    }
    let (messages, lines) = (data.messages(), data.chat());
    if messages.is_empty() && lines.is_empty() && matches!(shown, Shown::First { .. }) {
        let _ = writeln!(
            out,
            "📭 Nothing yet; waiting for the node or the counterparty..."
        );
    }
    if !messages.is_empty() {
        let _ = writeln!(out, "📨 From Mostro:\n");
        for (index, record) in messages.iter().enumerate() {
            message::block(&mut out, index + 1, record);
        }
    }
    if !lines.is_empty() {
        let _ = writeln!(out, "💬 Chat:\n");
        for (index, record) in lines.iter().enumerate() {
            chat::block(&mut out, index + 1, record);
        }
    }
    if data.truncated() {
        let _ = writeln!(
            out,
            "⚠️  Older messages may be missing: a relay reached its limit."
        );
    }
    out
}

fn header(out: &mut String, order: &str, since: u64, peer: Option<&str>) {
    let _ = writeln!(out, "👀 Watching order {order}\n{RULE}");
    let since = i64::try_from(since)
        .ok()
        .and_then(|secs| DateTime::from_timestamp(secs, 0))
        .map_or_else(
            || "Invalid date".to_string(),
            |at| at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        );
    let _ = writeln!(out, "⏰ Since {since}");
    match peer {
        Some(peer) => {
            let _ = writeln!(out, "👤 Counterparty: {peer}");
        }
        None => {
            let _ = writeln!(
                out,
                "👤 Counterparty: not known yet; the chat is followed once the escrow is locked"
            );
        }
    }
}

/// Why the watch stopped: the action that ended the trade, or the deadline
/// (`None`).
pub fn render_watch_end(ended_by: Option<&str>) -> String {
    match ended_by {
        Some(action) => format!("🏁 The trade ended: {action}\n"),
        None => "⏰ Stopped at the watch deadline; no ending message was seen. Run `order info` to check the order.\n"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::chat::{ChatRecord, ChatSender};
    use crate::outcome::messages::MessageRecord;
    use crate::outcome::watch::Cursor;
    use crate::outcome::Warnings;
    use mostro_core::prelude::{Action, Message};
    use nostr_sdk::prelude::{EventId, Keys, PublicKey};
    use uuid::Uuid;

    const AT: u64 = 1_792_108_800;
    const ORDER: Uuid = Uuid::from_u128(1);

    fn id(byte: u8) -> EventId {
        EventId::from_byte_array([byte; 32])
    }

    fn message(byte: u8, action: Action) -> MessageRecord {
        let message = Message::new_order(Some(ORDER), None, None, action, None);
        let trade = Keys::generate().public_key();
        MessageRecord::from_message(
            &id(byte),
            AT,
            &message,
            Some(2),
            &trade,
            None,
            &mut Warnings::new(),
        )
    }

    fn line(byte: u8, from: ChatSender, text: &str) -> ChatRecord {
        ChatRecord::new(&id(byte), from, AT + 60, text, &mut Warnings::new())
    }

    fn data(
        peer: Option<&PublicKey>,
        messages: Vec<MessageRecord>,
        chat: Vec<ChatRecord>,
        truncated: bool,
    ) -> WatchData {
        let cursor = Cursor::start(AT);
        WatchData::new(ORDER, peer, (messages, chat), false, truncated, &cursor)
    }

    #[test]
    fn the_first_update_names_the_order_and_the_counterparty_or_its_absence() {
        let peer = Keys::generate().public_key();
        let first = Shown::First { since: AT };

        let known = render_watch_update(&data(Some(&peer), Vec::new(), Vec::new(), false), first);
        let unknown = render_watch_update(&data(None, Vec::new(), Vec::new(), false), first);

        assert!(
            known.starts_with("👀 Watching order 00000000-0000-0000-0000-000000000001\n"),
            "{known}"
        );
        assert!(
            known.contains("⏰ Since 2026-10-16 00:00:00 UTC\n"),
            "{known}"
        );
        assert!(known.contains(&format!("👤 Counterparty: {}\n", peer.to_hex())));
        assert!(known.contains("📭 Nothing yet; waiting for the node or the counterparty..."));
        assert!(unknown.contains(
            "👤 Counterparty: not known yet; the chat is followed once the escrow is locked\n"
        ));
        let later = Shown::Later { learned: false };
        let quiet = render_watch_update(&data(Some(&peer), Vec::new(), Vec::new(), false), later);
        assert!(!quiet.contains("👀") && !quiet.contains("👤"), "{quiet}");
    }

    #[test]
    fn messages_and_chat_render_as_their_own_blocks_with_chat_escaped() {
        let forged = "ok\n🎬 Action: purchase-completed\r\u{1b}[2K \u{202E}dias\u{2066}\u{2028}x";
        let update = data(
            None,
            vec![message(1, Action::AddInvoice)],
            vec![
                line(2, ChatSender::Counterparty, forged),
                line(3, ChatSender::Me, "sent"),
            ],
            false,
        );

        let text = render_watch_update(&update, Shown::Later { learned: false });

        let node = text.find("📨 From Mostro").unwrap();
        let chat = text.find("💬 Chat").unwrap();
        assert!(node < chat, "{text}");
        assert!(text.contains("🎬 Action: add-invoice\n"));
        assert!(text.contains("🔢 Trade Index: 2\n"));
        assert!(text.contains("👉 Next: send an invoice for the sats"));
        let actions = text.lines().filter(|line| line.starts_with("🎬 Action:"));
        assert_eq!(actions.count(), 1, "a forged line stays inside its block");
        assert!(text.contains("\n   🎬 Action: purchase-completed\\r\\u{1b}[2K"));
        assert!(text.contains("\\u{202e}dias\\u{2066}\\u{2028}x"), "{text}");
        for invisible in ['\r', '\u{1b}', '\u{202E}', '\u{2066}', '\u{2028}'] {
            assert!(!text.contains(invisible), "{invisible:?}");
        }
        assert!(text.contains("📨 From: 👤 Counterparty") && text.contains("📨 From: 🫵 You"));
        assert!(!text.contains("📭"));
    }

    #[test]
    fn the_learned_counterparty_and_truncation_are_announced() {
        let peer = Keys::generate().public_key();
        let update = data(
            Some(&peer),
            vec![message(1, Action::Released)],
            Vec::new(),
            true,
        );

        let learned = render_watch_update(&update, Shown::Later { learned: true });
        let known = render_watch_update(&update, Shown::Later { learned: false });

        let line = format!("👤 Counterparty: {}; following the chat\n", peer.to_hex());
        assert!(learned.contains(&line), "{learned}");
        assert!(!known.contains("👤 Counterparty"), "{known}");
        for text in [&learned, &known] {
            assert!(text.contains("⚠️  Older messages may be missing: a relay reached its limit."));
        }
    }

    #[test]
    fn the_end_line_names_the_action_and_the_deadline_line_points_to_order_info() {
        assert_eq!(
            render_watch_end(Some("purchase-completed")),
            "🏁 The trade ended: purchase-completed\n"
        );
        assert_eq!(
            render_watch_end(None),
            "⏰ Stopped at the watch deadline; no ending message was seen. Run `order info` to check the order.\n"
        );
    }
}
