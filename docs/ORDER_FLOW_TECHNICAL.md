# Order Creation: Technical Details

This document covers the internal implementation, code flow, and Nostr message structures for creating orders.

## Code Flow Implementation

The core logic resides in `src/cli/new_order.rs`. Here is the step-by-step process:

1. **Validation**: Check if the fiat currency is supported (if market price is used) via external APIs (e.g., Yadio).
2. **Key Derivation**: 
   - Fetch identity keys for signing messages.
   - Generate **new** trade keys for this specific order using `User::get_next_trade_keys()`.
3. **Message Construction**: Build a `Message::Order` with `Action::NewOrder`.
4. **Encryption (NIP-59)**: Wrap the message in a Gift Wrap event to ensure the sender's identity is hidden from relays.
5. **Nostr Transmission**: Send the event to relays and subscribe to the trade key's pubkey to wait for Mostro's response.
6. **Local Persistence**: Save the order and its private trade key to the local SQLite database.

### Core Message Structure
```json
{
  "order": {
    "version": 1,
    "id": "eb5740f6-e584-46c5-953a-29bc3eb818f0",
    "request_id": 123456,
    "trade_index": 5,
    "action": "new-order",
    "payload": {
      "order": {
        "id": "eb5740f6-e584-46c5-953a-29bc3eb818f0",
        "kind": "sell",
        "status": "pending",
        "amount": 50000,
        "fiat_code": "USD",
        "fiat_amount": 100,
        "payment_method": "PayPal"
      }
    }
  }
}
```

## Nostr Event Types

Mostro uses two types of events during creation:

### 1. NIP-59 Gift Wrap (Private)
Used for the initial communication between the CLI and Mostro coordinator. It provides forward secrecy and protects sender metadata.

### 2. NIP-33 Parameterized Replaceable Event (Public)
Mostro publishes the order publicly using `kind: 38383` (defined by `NOSTR_ORDER_EVENT_KIND` in `mostro-core`). This event contains all order details in tags:
- `d`: Order ID (unique identifier)
- `k`: Kind (buy/sell)
- `s`: Status (pending)
- `f`: Fiat code
- `amt`: Satoshis
- `fa`: Fiat amount
- `y`: "mostro" (application identifier)
- `z`: "order" (entity identifier)

## Implementation Reference
- `src/cli/new_order.rs`: Main command handler.
- `src/util/messaging.rs`: Logic for Gift Wrap and encrypted DM sending.
- `src/db.rs`: Methods for saving order state and managing trade indices.
