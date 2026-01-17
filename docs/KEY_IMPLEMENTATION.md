# Key Management Implementation

This document provides technical details, code examples, and security practices for Mostro CLI key management.

## Database Storage

### User Table

Stores the root secret and identity info.

```sql
CREATE TABLE users (
    i0_pubkey char(64) PRIMARY KEY,  -- Identity public key (hex)
    mnemonic TEXT,                    -- BIP-39 mnemonic
    last_trade_index INTEGER,         -- Last used trade index
    created_at INTEGER
);
```

### Order Table

Stores trade-specific keys.

```sql
CREATE TABLE orders (
    id TEXT PRIMARY KEY,
    trade_keys TEXT,      -- Private key for this trade (hex)
    -- ... other fields
);
```

## Implementation Details

### Deriving Identity Keys

```rust
pub async fn get_identity_keys(pool: &SqlitePool) -> Result<Keys> {
    let user = User::get(pool).await?;
    let account = NOSTR_ORDER_EVENT_KIND as u32; // 38383
    
    Keys::from_mnemonic_advanced(
        &user.mnemonic,
        None,
        Some(account),
        Some(0),
        Some(0) // Identity is always index 0
    )
}
```

### Deriving Trade Keys

```rust
pub async fn get_trade_keys(pool: &SqlitePool, index: i64) -> Result<Keys> {
    let user = User::get(pool).await?;
    let account = NOSTR_ORDER_EVENT_KIND as u32;
    
    Keys::from_mnemonic_advanced(
        &user.mnemonic,
        None,
        Some(account),
        Some(0),
        Some(index as u32) // Incremental index 1, 2, 3...
    )
}
```

## Security Best Practices

### DO ✅

- **Use unique keys**: Always use `get_next_trade_keys()` for new orders.
- **Sign with identity**: Prove authenticity via NIP-59 seal (encrypted, never publicly revealed) while maintaining sender privacy.
- **Update indices**: Ensure `last_trade_index` is updated after successful creation.

### DON'T ❌

- **Reuse keys**: Never use the same trade key for two different orders.
- **Author with identity**: Never set the `pubkey` of a public event to your identity key.
- **Lose mnemonic**: Keys cannot be recovered without the seed phrase.

## Key Recovery

If the local database is lost but the mnemonic is saved:

1. **Identity**: Re-deriving index 0 restores the original `npub`.
2. **Trade History**: Re-deriving indices 1, 2, 3... restores access to trade messages.
3. **Session Sync**: Use `mostro-cli restore` to fetch active orders and their associated trade indices from the Mostro coordinator.

## Troubleshooting

### "Cannot decrypt message"

Usually means the CLI is trying to use the wrong trade key. Ensure you are loading the key associated with the specific `order_id` from the database.

### "Trade index mismatch"

Occurs when the local database index is behind Mostro's records. Run `mostro-cli restore` to synchronize.
