# Message Protocol and Communication

This document explains how Mostro CLI communicates privately and securely using Nostr protocols.

## Encryption Standards

Mostro CLI implements two major Nostr Improvement Proposals (NIPs) for communication:

### 1. NIP-59: Gift Wrap
Used for all communications between the CLI and the Mostro coordinator.
- **Privacy**: Hides the sender's identity from relays.
- **Security**: Provides forward secrecy by using ephemeral keys for the wrapper.
- **Content**: The inner "rumor" event contains the actual Mostro message.

### 2. NIP-17: Private Direct Messages
Used for encrypted peer-to-peer communication within the Mostro ecosystem.
- **Encryption**: Uses NIP-44 v2 encryption.
- **Conversation Keys**: Derived from the sender's secret key and recipient's public key.

## Communication Flow

### Request-Response Pattern
1. **Subscribe**: The CLI subscribes to the trade key's pubkey on Nostr relays.
2. **Send**: The CLI sends a Gift Wrapped message to Mostro.
3. **Listen**: The CLI waits for an incoming Gift Wrapped event from Mostro.
4. **Unwrap**: The CLI decrypts the Gift Wrap and rumor to extract the message.

## Code Reference
- `src/util/messaging.rs`: Logic for wrapping and unwrapping events.
- `src/parser/dms.rs`: Logic for parsing decrypted Mostro messages.
- `src/util/events.rs`: Logic for creating Nostr filters.
