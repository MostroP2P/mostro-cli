## CLI Commands Reference

This document lists all `mostro-cli` subcommands defined in `src/cli.rs`, their main arguments, and the Rust handler that implements each command. Use this when wiring new commands or changing existing flows.

All commands are part of the `Commands` enum and are dispatched via `Commands::run(&self, ctx: &Context)`.

### Orders

- **`listorders`**
  - **Description**: Requests open orders from the configured Mostro pubkey.
  - **Args**:
    - `--status <STRING>`: Optional order status filter.
    - `--currency <STRING>`: Optional fiat currency code.
    - `--kind <STRING>`: Optional order kind (buy/sell).
  - **Handler**: `execute_list_orders(kind, currency, status, ctx)` in `src/cli/list_orders.rs`.

- **`neworder`**
  - **Description**: Create a new buy/sell order on Mostro.
  - **Args**:
    - `--kind <STRING>`: Order kind (e.g. `buy` or `sell`).
    - `--amount <i64>`: Sats amount; `0` means market price.
    - `-c, --fiat-code <STRING>`: Fiat currency code.
    - `--fiat-amount <RANGE>`: Fiat amount or range, validated by `check_fiat_range`.
    - `-m, --payment-method <STRING>`: Payment method identifier.
    - `--premium <i64>`: Premium on the price (can be negative).
    - `--invoice <STRING>`: Optional Lightning invoice.
    - `--expiration-days <i64>`: Expiration time in days for pending orders.
  - **Handler**: `execute_new_order(...)` in `src/cli/new_order.rs`.

- **`takesell`**
  - **Description**: Take a sell order from a Mostro pubkey.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
    - `--invoice <STRING>`: Optional Lightning invoice.
    - `--amount <u32>`: Fiat amount to buy.
  - **Handler**: `execute_take_order(order_id, Action::TakeSell, invoice, amount, ctx)` in `src/cli/take_order.rs`.

- **`takebuy`**
  - **Description**: Take a buy order from a Mostro pubkey.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
    - `--amount <u32>`: Fiat amount to sell.
  - **Handler**: `execute_take_order(order_id, Action::TakeBuy, &None, amount, ctx)` in `src/cli/take_order.rs`.

- **`addinvoice`**
  - **Description**: Buyer adds a new invoice to receive the payment.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
    - `--invoice <STRING>`: Lightning invoice.
  - **Handler**: `execute_add_invoice(order_id, invoice, ctx)` in `src/cli/add_invoice.rs`.

- **`fiatsent`**
  - **Description**: Send a "fiat sent" message to confirm payment to the counterparty.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
  - **Handler**: `util::run_simple_order_msg(Commands::FiatSent { .. }, Some(order_id), ctx)`.

- **`release`**
  - **Description**: Settle the hold invoice and pay to the buyer.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
  - **Handler**: `util::run_simple_order_msg(Commands::Release { .. }, Some(order_id), ctx)`.

- **`cancel`**
  - **Description**: Cancel a pending order.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
  - **Handler**: `util::run_simple_order_msg(Commands::Cancel { .. }, Some(order_id), ctx)`.

- **`ordersinfo`**
  - **Description**: Request detailed information for specific orders.
  - **Args**:
    - `--order-ids <UUID>...`: One or more order IDs.
  - **Handler**: `execute_orders_info(order_ids, ctx)` in `src/cli/orders_info.rs`.

### Disputes

- **`dispute`**
  - **Description**: Start a dispute for an order.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
  - **Handler**: `util::run_simple_order_msg(Commands::Dispute { .. }, Some(order_id), ctx)`.

- **`listdisputes`**
  - **Description**: Request open disputes from the Mostro pubkey.
  - **Args**: None.
  - **Handler**: `execute_list_disputes(ctx)` in `src/cli/list_disputes.rs`.

### Direct messages (user)

- **`getdm`**
  - **Description**: Get the latest direct messages.
  - **Args**:
    - `--since <i64>`: Minutes back from now to query (default: 30).
    - `--from-user`: If true, get messages from the counterparty instead of Mostro.
  - **Handler**: `execute_get_dm(since, false, from_user, ctx)` in `src/cli/get_dm.rs`.

- **`getdmuser`**
  - **Description**: Get direct messages sent to any trade keys.
  - **Args**:
    - `--since <i64>`: Minutes back from now to query (default: 30).
  - **Handler**: `execute_get_dm_user(since, ctx)` in `src/cli/get_dm_user.rs`.

- **`senddm`**
  - **Description**: Send a direct message to a user.
  - **Args**:
    - `--pubkey <NPUB/HEX>`: Pubkey of the counterpart.
    - `--order-id <UUID>`: Order identifier (for context).
    - `--message <STRING>...`: Message parts; joined with spaces.
  - **Handler**: `execute_send_dm(PublicKey::from_str(pubkey)?, ctx, order_id, &msg)` in `src/cli/send_dm.rs`.

- **`dmtouser`**
  - **Description**: Send a direct message to a user via a **shared-key custom wrap**. Derives an ECDH shared key from the order’s trade keys and the recipient pubkey; the message is sent as a NIP-59 gift wrap addressed to the shared key’s public key (NIP-44 encrypted), so both sides can decrypt.
  - **Args**:
    - `--pubkey <NPUB/HEX>`: Recipient pubkey.
    - `--order-id <UUID>`: Order id to derive trade keys and shared key.
    - `--message <STRING>...`: Message parts; joined with spaces.
  - **Handler**: `execute_dm_to_user(PublicKey::from_str(pubkey)?, &ctx.client, order_id, &msg, &ctx.pool)` in `src/cli/dm_to_user.rs`.

### Direct messages (admin / solver)

- **`getadmindm`**
  - **Description**: Get the latest direct messages for admin.
  - **Args**:
    - `--since <i64>`: Minutes back from now to query (default: 30).
    - `--from-user`: If true, get messages from the counterparty instead of Mostro.
  - **Handler**: `execute_get_dm(since, true, from_user, ctx)` in `src/cli/get_dm.rs`.

- **`admsenddm`** *(admin only)*
  - **Description**: Send a gift-wrapped direct message to a user as admin/solver.
  - **Args**:
    - `--pubkey <NPUB/HEX>`: Recipient pubkey.
    - `--message <STRING>...`: Message parts; joined with spaces.
  - **Handler**: `execute_adm_send_dm(PublicKey::from_str(pubkey)?, ctx, &msg)` in `src/cli/adm_send_dm.rs`.

- **`sendadmindmattach`** *(admin only)*
  - **Description**: Send an admin DM with an encrypted attachment stored on a Blossom server.
  - **Args**:
    - `--pubkey <NPUB/HEX>`: Admin recipient pubkey.
    - `--order-id <UUID>`: Order id to derive the correct trade key.
    - `--file <PATH>`: Path to the file to encrypt and upload.
  - **Handler**: `execute_send_admin_dm_attach(PublicKey::from_str(pubkey)?, ctx, order_id, file)` in `src/cli/send_admin_dm_attach.rs`.

### Identity & keys

- **`conversationkey`**
  - **Description**: Get the conversation key for direct messaging with a user.
  - **Args**:
    - `--pubkey <NPUB/HEX>`: Counterparty pubkey.
  - **Handler**: `execute_conversation_key(&ctx.trade_keys, PublicKey::from_str(pubkey)?)` in `src/cli/conversation_key.rs`.

- **`getlasttradeindex`**
  - **Description**: Get last trade index of the user.
  - **Args**: None.
  - **Handler**: `execute_last_trade_index(&ctx.identity_keys, ctx.mostro_pubkey, ctx)` in `src/cli/last_trade_index.rs`.

- **`getlasttradeprivkey`**
  - **Description**: Get private key of the last trade index public key.
  - **Args**: None.
  - **Handler**: `execute_last_trade_index_private_key(ctx)` in `src/cli/last_trade_index.rs`.

### Session & restore

- **`restore`**
  - **Description**: Restore session to recover all pending orders and disputes.
  - **Args**: None.
  - **Handler**: `execute_restore(&ctx.identity_keys, ctx.mostro_pubkey, ctx)` in `src/cli/restore.rs`.

### Admin / solver dispute management

- **`admcancel`** *(admin only)*
  - **Description**: Cancel a dispute.
  - **Args**:
    - `--dispute-id <UUID>`: Dispute identifier.
  - **Handler**: `execute_admin_cancel_dispute(order_id, ctx)` in `src/cli/take_dispute.rs`.

- **`admsettle`** *(admin only)*
  - **Description**: Settle a dispute.
  - **Args**:
    - `--dispute-id <UUID>`: Dispute identifier.
  - **Handler**: `execute_admin_settle_dispute(order_id, ctx)` in `src/cli/take_dispute.rs`.

- **`admaddsolver`** *(admin only)*
  - **Description**: Add a new dispute solver.
  - **Args**:
    - `--npubkey <NPUB>`: Nostr pubkey of the solver.
  - **Handler**: `execute_admin_add_solver(npubkey, ctx)` in `src/cli/take_dispute.rs`.

- **`admtakedispute`** *(admin/solver only)*
  - **Description**: Admin or solver takes a pending dispute.
  - **Args**:
    - `--dispute-id <UUID>`: Dispute identifier.
  - **Handler**: `execute_take_dispute(dispute_id, ctx)` in `src/cli/take_dispute.rs`.

### Rating

- **`rate`**
  - **Description**: Rate counterpart after a successful trade.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
    - `--rating <u8>`: Rating from 1 to 5.
  - **Handler**: `execute_rate_user(order_id, rating, ctx)` in `src/cli/rate_user.rs`.

---

If you add a new variant to the `Commands` enum:

1. Add the subcommand and its arguments in `src/cli.rs`.
2. Add its handler function in an appropriate `src/cli/*.rs` module.
3. Extend the `Commands::run` match to call the handler.
4. Update this `commands.md` file so documentation stays in sync for humans and AI tools.

