//! Human presentation of `order.chat`: the conversation oldest first with who
//! wrote each message, then the message sent on this run. Built from the typed
//! outcome only. Chat text is untrusted: each line is escaped, so it cannot
//! drive the terminal, and indented, which no longer sets it apart from our
//! lines once the terminal wraps a long line to column 0.

use std::fmt::Write;

use super::table::escape_untrusted;
use crate::outcome::chat::{ChatData, ChatRecord, ChatSender};

const RULE: &str = "═══════════════════════════════════════";
const THIN_RULE: &str = "─────────────────────────────────────";
const INDENT: &str = "   ";

/// The conversation of one order, and the message sent on this run.
pub fn render_chat(data: &ChatData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "💬 Order Chat\n{RULE}");
    let _ = writeln!(out, "📋 Order: {}", data.order_id());
    let _ = writeln!(out, "👤 Counterparty: {}", data.counterparty_pubkey());
    if data.messages().is_empty() {
        let _ = writeln!(out, "📭 No chat messages");
    } else {
        let _ = writeln!(out, "📊 {} message(s):\n", data.messages().len());
        for (index, message) in data.messages().iter().enumerate() {
            block(&mut out, index + 1, message);
        }
    }
    if data.truncated() {
        let _ = writeln!(
            out,
            "⚠️  Older messages may be missing: the read reached its limit of 200."
        );
    }
    if let Some(sent) = data.sent() {
        let _ = writeln!(out, "📤 Your message (id {}):", sent.message_id());
        text(&mut out, sent.text());
    }
    out
}

fn block(out: &mut String, number: usize, message: &ChatRecord) {
    let _ = writeln!(out, "📄 Message {number}:\n{THIN_RULE}");
    let time = message.created_at().map_or_else(
        || "Invalid date".to_string(),
        |at| at.replace('T', " ").replace('Z', " UTC"),
    );
    let _ = writeln!(out, "📅 Time: {time}");
    let from = match message.from() {
        ChatSender::Me => "🫵 You",
        ChatSender::Counterparty => "👤 Counterparty",
    };
    let _ = writeln!(out, "📨 From: {from}\n📝 Text:");
    text(out, message.text());
    out.push('\n');
}

/// Each line of `text`, escaped and indented.
fn text(out: &mut String, text: &str) {
    for line in text.split('\n') {
        let _ = writeln!(out, "{INDENT}{}", escape_untrusted(line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Warnings;
    use nostr_sdk::prelude::{EventId, Keys};
    use uuid::Uuid;

    const AT: u64 = 1_792_108_800;

    fn record(byte: u8, from: ChatSender, at: u64, text: &str) -> ChatRecord {
        ChatRecord::new(
            &EventId::from_byte_array([byte; 32]),
            from,
            at,
            text,
            &mut Warnings::new(),
        )
    }

    fn data(messages: Vec<ChatRecord>, truncated: bool, sent: Option<ChatRecord>) -> ChatData {
        ChatData::new(
            Uuid::from_u128(1),
            &Keys::generate().public_key(),
            (messages, truncated),
            sent,
        )
    }

    #[test]
    fn the_conversation_reads_oldest_first_with_who_wrote_each_message() {
        let chat = data(
            vec![
                record(1, ChatSender::Me, AT, "fiat sent"),
                record(2, ChatSender::Counterparty, AT + 60, "got it"),
            ],
            false,
            None,
        );

        let text = render_chat(&chat);

        assert!(text.starts_with("💬 Order Chat\n"), "{text}");
        assert!(text.contains("📋 Order: 00000000-0000-0000-0000-000000000001"));
        assert!(text.contains(&format!("👤 Counterparty: {}", chat.counterparty_pubkey())));
        assert!(text.contains("📊 2 message(s):"));
        let mine = text.find("📨 From: 🫵 You").unwrap();
        let theirs = text.find("📨 From: 👤 Counterparty").unwrap();
        assert!(mine < theirs, "oldest first: {text}");
        assert!(text.contains("📅 Time: 2026-10-16 00:00:00 UTC"));
        assert!(text.contains("\n   fiat sent\n") && text.contains("\n   got it\n"));
        assert!(!text.contains("📭") && !text.contains("📤") && !text.contains("⚠️"));
    }

    #[test]
    fn an_empty_conversation_says_so() {
        let text = render_chat(&data(Vec::new(), false, None));
        assert!(text.contains("📭 No chat messages"), "{text}");
        assert!(!text.contains("📄 Message"));
    }

    #[test]
    fn a_forged_line_and_bidi_controls_are_escaped_and_indented() {
        let forged = "ok\n📨 From: 🫵 You\r📤 Your message\u{1b}[2K \u{202E}dias\u{2066}\u{2028}x";
        let text = render_chat(&data(
            vec![record(1, ChatSender::Counterparty, AT, forged)],
            false,
            None,
        ));

        let from_lines = text.lines().filter(|line| line.starts_with("📨 From:"));
        assert_eq!(from_lines.count(), 1, "{text}");
        assert!(text.contains("\n   📨 From: 🫵 You\\r📤 Your message\\u{1b}[2K"));
        assert!(text.contains("\\u{202e}dias\\u{2066}\\u{2028}x"), "{text}");
        for invisible in ['\r', '\u{1b}', '\u{202E}', '\u{2066}', '\u{2028}'] {
            assert!(!text.contains(invisible), "{invisible:?}");
        }
        assert!(!text.lines().any(|line| line.starts_with("📤")));
    }

    #[test]
    fn the_sent_message_and_the_truncation_are_shown_apart() {
        let sent = record(9, ChatSender::Me, AT + 120, "on my way\nsee you");
        let text = render_chat(&data(
            vec![record(1, ChatSender::Counterparty, u64::MAX, "old")],
            true,
            Some(sent.clone()),
        ));

        assert!(text.contains("📅 Time: Invalid date"), "{text}");
        assert!(text.contains("📊 1 message(s):"));
        assert!(text.contains("⚠️  Older messages may be missing"));
        let block = format!(
            "📤 Your message (id {}):\n   on my way\n   see you\n",
            sent.message_id()
        );
        assert!(text.ends_with(&block), "{text}");
    }
}
