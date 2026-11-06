//! Human presentation of `identity.init` and `identity.show`. Built from the
//! typed outcome only, which holds public values only; the mnemonic is never
//! shown.

use std::fmt::Write;

use crate::outcome::identity::{IdentityInitData, IdentityShowData};

const RULE: &str = "═══════════════════════════════════════";

/// The identity a new store was created with.
pub fn render_identity_init(data: &IdentityInitData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "🪪 Identity Created\n{RULE}");
    let _ = writeln!(out, "🔑 Identity Public Key: {}", data.identity_pubkey());
    let _ = writeln!(
        out,
        "🔒 The mnemonic stays in the local store and is never shown."
    );
    out
}

/// The identity of the existing store.
pub fn render_identity_show(data: &IdentityShowData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "🪪 Local Identity\n{RULE}");
    let _ = writeln!(out, "🔑 Identity Public Key: {}", data.identity_pubkey());
    let _ = writeln!(out, "🔢 Last Trade Index: {}", data.last_trade_index());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::Keys;

    #[test]
    fn both_show_the_public_key_and_show_the_index() {
        let key = Keys::generate().public_key();
        let init = render_identity_init(&IdentityInitData::new(&key));
        assert!(
            init.contains(&key.to_hex()) && init.contains("never shown"),
            "{init}"
        );
        let show = render_identity_show(&IdentityShowData::new(&key, 4));
        assert!(show.contains(&key.to_hex()), "{show}");
        assert!(show.contains("Last Trade Index: 4"), "{show}");
    }
}
