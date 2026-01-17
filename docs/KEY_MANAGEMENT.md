# Key Management in Mostro CLI

This document explains how Mostro CLI manages cryptographic keys for identity and trading.

## Overview

Mostro CLI uses **hierarchical deterministic (HD) keys** following the BIP-32/44 standards. This allows deriving multiple keys from a single mnemonic seed phrase while maintaining user privacy across trades.

## Key Hierarchy

```text
Master Seed (BIP-39 mnemonic - 12 words)
    │
    └─ m/44'/1237'/38383'/0/
           ├─ 0  → Identity Key (permanent, for signing)
           ├─ 1  → Trade Key #1 (ephemeral, for first trade)
           ├─ 2  → Trade Key #2 (ephemeral, for second trade)
           ├─ 3  → Trade Key #3 (ephemeral, for third trade)
           └─ n  → Trade Key #n (ephemeral, for nth trade)
```

### Path Components

- **Purpose**: `44'` - BIP-44 standard
- **Coin Type**: `1237'` - Nostr coin type
- **Account**: `38383'` - Mostro order event kind (`NOSTR_ORDER_EVENT_KIND`)
- **Change**: `0` - External chain
- **Index**: `0` for identity, `1+` for trades

## Identity Keys vs Trade Keys

### Identity Keys (Index 0)

**Purpose**: Represents the user's persistent identity.

**Characteristics**:

- **Permanent**: Never changes, created once at initialization.
- **Used for**: Signing messages to prove authenticity to Mostro.
- **Not used as**: Event author (for privacy).
- **Stored as**: Public key in database (`i0_pubkey`).

### Trade Keys (Index 1, 2, 3, ...)

**Purpose**: Ephemeral keys used for each individual trade to maintain privacy.

**Characteristics**:

- **Ephemeral**: New key for each trade.
- **Used for**:
  - Authoring Nostr events (as the sender).
  - Receiving encrypted messages.
  - Trade-specific communications.
- **Privacy**: Counterparty cannot link trades together.
- **Stored**: Private key stored with each order in database.

## Why Two Types of Keys?

### Privacy Through Separation

Without trade keys, all orders would be linked to one identity:

```text
❌ Without trade keys (bad for privacy):
Order #1 → User's Identity Key (npub1abc...)
Order #2 → User's Identity Key (npub1abc...)
Order #3 → User's Identity Key (npub1abc...)

Result: Anyone can see all orders from same user!
```

With trade keys, each order appears independent:

```text
✅ With trade keys (good for privacy):
Order #1 → Trade Key #1 (npub1xyz...) + signed by identity
Order #2 → Trade Key #2 (npub1def...) + signed by identity
Order #3 → Trade Key #3 (npub1ghi...) + signed by identity

Result: Orders appear unrelated! Privacy maintained.
```

### Authenticity Through Signing

The identity key signs messages to prove they're from the real user. Mostro can verify:

1. ✅ Message came from a legitimate user (signature verification).
2. ✅ User's reputation/history (identity-based).
3. ✅ Each trade maintains privacy (separate trade keys).

## Key Usage Patterns

- **New Order**: Generate new trade key using `get_next_trade_keys()`.
- **Existing Order**: Retrieve stored trade key from the local database.
- **Always Sign**: Use identity key for the `Message::sign` payload.

For implementation details and code examples, see [KEY_IMPLEMENTATION.md](./KEY_IMPLEMENTATION.md).
