//! Payer canonicalisation for `declare-payer` (protocol book,
//! `payer_declaration.md`, "Canonicalisation and hash").
//!
//! Mostro never sees the payer details: the buyer's client turns them into a
//! canonical string, hashes it with [`mostro_core::payer::payment_hash`] and
//! sends only the hash. Every client must build the same string from the
//! same account, so the rules here follow the protocol's method registry
//! exactly.

use anyhow::{bail, Result};
use unicode_normalization::UnicodeNormalization;

/// How a registry field is normalised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    /// IBAN, CBU/CVU, account number, tax id: whitespace, `-`, `.` and `/`
    /// removed.
    Identifier,
    /// Holder name: whitespace runs collapsed to one space and trimmed.
    Name,
}

/// A payment method from the protocol's registry. PIX is deliberately
/// absent: a PIX key names the receiving account, so the seller cannot match
/// a buyer's key against the payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayerMethod {
    /// `AR|CVU`: CBU or CVU number, then the holder's CUIT/CUIL.
    ArCvu,
    /// `EU|SEPA`: IBAN, then the account holder name.
    EuSepa,
}

impl PayerMethod {
    /// Every registered method, for help texts and errors.
    pub const ALL: [PayerMethod; 2] = [PayerMethod::ArCvu, PayerMethod::EuSepa];

    /// The `<COUNTRY>|<METHOD>` prefix.
    pub fn prefix(self) -> &'static str {
        match self {
            PayerMethod::ArCvu => "AR|CVU",
            PayerMethod::EuSepa => "EU|SEPA",
        }
    }

    /// Human description of the fields, in order.
    pub fn fields_help(self) -> &'static str {
        match self {
            PayerMethod::ArCvu => "CBU/CVU number, holder CUIT/CUIL",
            PayerMethod::EuSepa => "IBAN, account holder name",
        }
    }

    fn field_kinds(self) -> &'static [FieldKind] {
        match self {
            PayerMethod::ArCvu => &[FieldKind::Identifier, FieldKind::Identifier],
            PayerMethod::EuSepa => &[FieldKind::Identifier, FieldKind::Name],
        }
    }

    /// Parse a registry prefix such as `AR|CVU`, `ar-cvu` or `EU/SEPA`.
    pub fn parse(value: &str) -> Result<Self> {
        let wanted: String = value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_uppercase();
        for method in Self::ALL {
            if method.prefix().replace('|', "") == wanted {
                return Ok(method);
            }
        }
        let known: Vec<&str> = Self::ALL.iter().map(|m| m.prefix()).collect();
        bail!(
            "unknown payment method {value:?}; registered methods: {}",
            known.join(", ")
        )
    }
}

fn base(value: &str) -> String {
    value.nfkc().collect::<String>().to_uppercase()
}

fn normalise(kind: FieldKind, value: &str) -> String {
    let upper = base(value);
    match kind {
        FieldKind::Identifier => upper
            .chars()
            .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '.' | '/'))
            .collect(),
        FieldKind::Name => upper.split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// Whether every code point of a normalised field lies in the protocol's
/// repertoire, U+0020–U+007E and U+00A0–U+017F, where NFKC and the default
/// uppercase mapping are the same in every Unicode version.
fn in_repertoire(field: &str) -> bool {
    field
        .chars()
        .all(|c| matches!(c, '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{17f}'))
}

/// The canonical string for `fields` under `method`, or an error when the
/// payer has no canonical form (wrong field count, a field empty after
/// normalisation, one containing `|`, or one outside the repertoire).
pub fn canonical_payer(method: PayerMethod, fields: &[String]) -> Result<String> {
    let kinds = method.field_kinds();
    if fields.len() != kinds.len() {
        bail!(
            "{} needs {} field(s) ({}), got {}",
            method.prefix(),
            kinds.len(),
            method.fields_help(),
            fields.len()
        );
    }
    let mut parts = vec![method.prefix().to_string()];
    for (kind, raw) in kinds.iter().zip(fields) {
        let field = normalise(*kind, raw);
        if field.is_empty() || field.contains('|') {
            bail!("{raw:?} has no canonical form (empty or contains '|')");
        }
        if !in_repertoire(&field) {
            bail!(
                "{raw:?} has no canonical form: only Latin letters (U+0020-007E, \
                 U+00A0-017F) are allowed"
            );
        }
        parts.push(field);
    }
    Ok(parts.join("|"))
}

/// Suggested display tier of a [`PaymentHistory`] (protocol book, "Suggested
/// tiers"). Client policy, not protocol: it never decides anything, it only
/// labels what the seller sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryTier {
    Established,
    Limited,
    New,
    Unavailable,
}

impl HistoryTier {
    /// Wording the protocol suggests for the tier.
    pub fn label(self) -> &'static str {
        match self {
            HistoryTier::Established => "🟢 Established payment account",
            HistoryTier::Limited => "🟡 Limited payment history",
            HistoryTier::New => "🔴 No previous successful trades with this account",
            HistoryTier::Unavailable => "⚪ History unavailable for this buyer",
        }
    }
}

const ESTABLISHED_MIN_TRADES: u32 = 5;
const ESTABLISHED_MIN_COUNTERPARTIES: u32 = 3;
const ESTABLISHED_MIN_EXPERIENCED: u32 = 1;
const ESTABLISHED_MIN_AGE_SECS: i64 = 30 * 86_400;

/// Tier of `history` at unix time `now`.
pub fn history_tier(history: &mostro_core::payer::PaymentHistory, now: i64) -> HistoryTier {
    use mostro_core::payer::BuyerMode;
    if history.buyer_mode != BuyerMode::Reputation {
        return HistoryTier::Unavailable; // full_privacy, or a mode we do not know
    }
    if history.successful_trades == 0 {
        return HistoryTier::New;
    }
    let old_enough = history
        .first_success_at
        .is_some_and(|first| now - first >= ESTABLISHED_MIN_AGE_SECS);
    if history.successful_trades >= ESTABLISHED_MIN_TRADES
        && history.distinct_counterparties >= ESTABLISHED_MIN_COUNTERPARTIES
        && history.experienced_counterparties >= ESTABLISHED_MIN_EXPERIENCED
        && old_enough
    {
        HistoryTier::Established
    } else {
        HistoryTier::Limited
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::payer::payment_hash;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn protocol_normalisation_vectors() {
        let cases = [
            (
                PayerMethod::ArCvu,
                s(&["0000003100012345678901", "27-12345678-9"]),
                "AR|CVU|0000003100012345678901|27123456789",
            ),
            (
                PayerMethod::EuSepa,
                s(&["de89 3704 0044 0532 0130 00", "  Alice   Smith "]),
                "EU|SEPA|DE89370400440532013000|ALICE SMITH",
            ),
            (
                PayerMethod::EuSepa,
                s(&["ES91 2100 0418 4502 0005 1332", "José  García"]),
                "EU|SEPA|ES9121000418450200051332|JOSÉ GARCÍA",
            ),
        ];
        for (method, fields, expected) in cases {
            assert_eq!(canonical_payer(method, &fields).unwrap(), expected);
        }
    }

    #[test]
    fn protocol_hash_vectors() {
        let cases = [
            (
                "AR|CVU|0000003100012345678901|27123456789",
                "df82c620ee9df8f7ad068e3bd771a707d9c0dfcfdef2c34a3fb8e6cdda3c9a2f",
            ),
            (
                "EU|SEPA|DE89370400440532013000|ALICE SMITH",
                "ee06af92c95429e7cb0cf8428636199a71a01e32bab7a8526d226161f0de9903",
            ),
            (
                "EU|SEPA|ES9121000418450200051332|JOSÉ GARCÍA",
                "91863709cf207cf042cece0cc4673241e4e0f321a39a327c16f05a9d0d231ebd",
            ),
        ];
        for (canonical, hash) in cases {
            assert_eq!(payment_hash(canonical), hash);
        }
    }

    #[test]
    fn decomposed_accents_normalise_to_the_composed_form() {
        // "E" + U+0301 must canonicalise like the single code point "É".
        let decomposed = s(&["ES91 2100 0418 4502 0005 1332", "Jose\u{301} García"]);
        assert_eq!(
            canonical_payer(PayerMethod::EuSepa, &decomposed).unwrap(),
            "EU|SEPA|ES9121000418450200051332|JOSÉ GARCÍA"
        );
    }

    #[test]
    fn whitespace_is_exactly_the_unicode_white_space_set() {
        // Protocol vectors: U+0085 and U+00A0 are whitespace, U+FEFF is not.
        let iban = "DE89 3704 0044 0532 0130 00";
        let spaced = canonical_payer(PayerMethod::EuSepa, &s(&[iban, "Alice\u{85}\u{a0}Smith"]));
        assert_eq!(
            spaced.unwrap(),
            "EU|SEPA|DE89370400440532013000|ALICE SMITH"
        );
        // U+FEFF is not whitespace, and outside the repertoire.
        assert!(canonical_payer(PayerMethod::EuSepa, &s(&[iban, "Alice\u{feff}Smith"])).is_err());
    }

    #[test]
    fn fields_are_limited_to_the_stable_latin_repertoire() {
        let iban = "DE89 3704 0044 0532 0130 00";
        let latin = canonical_payer(PayerMethod::EuSepa, &s(&[iban, "Groß  Łukasz"])).unwrap();
        assert_eq!(latin, "EU|SEPA|DE89370400440532013000|GROSS ŁUKASZ");
        assert_eq!(
            payment_hash(&latin),
            "4d5352f5235572ba4ddb61eefb1d294acde0424737d65721638da0d3b63676b6"
        );
        // U+0264 gained an uppercase in Unicode 16; Cyrillic is out of range.
        for name in ["\u{264}lice", "Алиса"] {
            assert!(
                canonical_payer(PayerMethod::EuSepa, &s(&[iban, name])).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn spaced_and_compact_identifiers_are_the_same_account() {
        let a = canonical_payer(
            PayerMethod::EuSepa,
            &s(&["DE89 3704 0044 0532 0130 00", "x"]),
        );
        let b = canonical_payer(PayerMethod::EuSepa, &s(&["DE89370400440532013000", "x"]));
        assert_eq!(a.unwrap(), b.unwrap());
    }

    #[test]
    fn payers_without_a_canonical_form_are_rejected() {
        assert!(canonical_payer(PayerMethod::EuSepa, &s(&[" - . / ", "x"])).is_err());
        assert!(canonical_payer(PayerMethod::EuSepa, &s(&["DE89", "A|B"])).is_err());
        assert!(canonical_payer(PayerMethod::ArCvu, &s(&["0000003100012345678901"])).is_err());
        assert!(canonical_payer(PayerMethod::EuSepa, &s(&["a", "b", "c"])).is_err());
    }

    #[test]
    fn method_prefixes_parse_leniently() {
        for v in ["AR|CVU", "ar-cvu", "arcvu", "AR/CVU"] {
            assert_eq!(PayerMethod::parse(v).unwrap(), PayerMethod::ArCvu);
        }
        assert_eq!(PayerMethod::parse("eu|sepa").unwrap(), PayerMethod::EuSepa);
        // Not in the registry: a PIX key is not sender data.
        assert!(PayerMethod::parse("BR|PIX").is_err());
        assert!(PayerMethod::parse("US|ZELLE").is_err());
    }

    fn history(
        mode: mostro_core::payer::BuyerMode,
        trades: u32,
        distinct: u32,
        experienced: u32,
        first: Option<i64>,
    ) -> mostro_core::payer::PaymentHistory {
        mostro_core::payer::PaymentHistory {
            payment_hash: "a".repeat(64),
            buyer_mode: mode,
            successful_trades: trades,
            distinct_counterparties: distinct,
            experienced_counterparties: experienced,
            first_success_at: first,
            last_success_at: first,
        }
    }

    #[test]
    fn history_tiers_follow_the_protocol_suggestion() {
        use mostro_core::payer::BuyerMode::*;
        let now = 1_000 * 86_400;
        let old = Some(now - 30 * 86_400);
        assert_eq!(
            history_tier(&history(Reputation, 5, 3, 1, old), now),
            HistoryTier::Established
        );
        // Each condition alone keeps the account at Limited.
        assert_eq!(
            history_tier(&history(Reputation, 4, 3, 1, old), now),
            HistoryTier::Limited
        );
        assert_eq!(
            history_tier(&history(Reputation, 5, 2, 1, old), now),
            HistoryTier::Limited
        );
        assert_eq!(
            history_tier(&history(Reputation, 5, 3, 0, old), now),
            HistoryTier::Limited
        );
        assert_eq!(
            history_tier(&history(Reputation, 5, 3, 1, Some(now - 29 * 86_400)), now),
            HistoryTier::Limited
        );
        assert_eq!(
            history_tier(&history(Reputation, 0, 0, 0, None), now),
            HistoryTier::New
        );
        assert_eq!(
            history_tier(&history(FullPrivacy, 0, 0, 0, None), now),
            HistoryTier::Unavailable
        );
        assert_eq!(
            history_tier(&history(Unknown, 9, 9, 9, old), now),
            HistoryTier::Unavailable
        );
    }
}
