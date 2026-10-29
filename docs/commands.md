## CLI Commands Reference

This document lists all `mostro-cli` subcommands defined in `src/cli/commands.rs`, their main arguments, and the Rust handler that implements each command. Use this when wiring new commands or changing existing flows.

All commands are part of the `Commands` enum and are dispatched via `Commands::run(&self, ctx: &Context)`.

### Global options

These flags apply to every subcommand (parsed on the top-level `Cli`; each also
settable via the matching env var):

- `-m, --mostro-pubkey <NPUB|HEX>` (`MOSTRO_PUBKEY`): the Mostro node to talk to. Required for every command except `schema`. The old spelling `--mostropubkey` was removed without an alias; scripts must switch.
- `-r, --relays <CSV>` (`RELAYS`): comma-separated relay URLs.
- `-p, --pow <BITS>` (`POW`): NIP-13 proof-of-work difficulty mined on outgoing events.
- `-s, --secret` (`SECRET=true`): full-privacy mode (unsigned inner tuple; identity = trade key).
- `-t, --transport <nip44>` (`TRANSPORT`): wire transport. Only `nip44`
  (protocol v2, signed kind 14) is supported.
  **Optional**: when omitted, the CLI auto-detects it at startup from the node's
  `protocol_version` tag on its kind-38385 info event (a node that advertises
  nothing is treated as `nip44`). A v1 node is an error.
- `-v, --verbose`: enable info-level logging (also surfaces the resolved transport).
- `--output <human|json>`: output format, accepted before, between or after
  command words (`--output json` and `--output=json`; case-sensitive, default
  `human` whatever the terminal, no environment variable). Only `order list`,
  `schema` and `order info` support `json`; on any other command it prints one JSON error outcome
  (`output_not_supported`, or `invalid_arguments` for bad arguments) on stdout
  and exits 2 before anything runs. The outcome's `command` is the canonical
  name from the [command registry](../specs/001-order-list-json/contracts/cli.md#canonical-command-names),
  or null when the command cannot be resolved.
  A missing, invalid or repeated `--output` exits 2 with nothing on stdout.

In human mode, argument errors exit 2 and print a fixed description on stderr,
such as `error: unexpected argument found`, without echoing the arguments given.
`--help` and `--version` print text and exit 0 even with `--output json`.

### Discovery

- **`schema`**
  - **Description**: Describes the machine interface of this build offline: the commands, which of them support `--output json`, their input and output schemas and the configuration names they read. It needs no node, relay, identity, database or stdin, and works with missing or invalid configuration.
  - **Args**: none (plus the global `--output`).
  - **Output**: human mode (the default) prints the raw discovery document as pretty JSON; `--output json` wraps the same data in the standard outcome with command `schema`.
  - **Handler**: `cli::schema::offline_discovery()` dispatched from `cli::run`, before any context, logger or RPC work.
  - **Generated reference**: [docs/schemas/discovery-v1.json](schemas/discovery-v1.json) is this command's output, and [docs/schemas/order-list-v1.json](schemas/order-list-v1.json) the `order.list` output schema. Both are generated and drift-tested; this prose is maintained by hand. Regenerate them after an intended change with `cargo run -q -- schema > docs/schemas/discovery-v1.json` and `cargo run -q -- --output json schema | jq '.data.commands[] | select(.name=="order.list") | .output_schema' > docs/schemas/order-list-v1.json`.

### Machine output (schema v1)

This applies to `order list`, `schema` and `order info`. Other commands print human text only. The errors below are those of the listing; `order info` adds its own codes, listed in its [contract](../specs/002-order-info-json/contracts/cli.md#stable-errors).

- **One document.** With `--output json`, a controlled outcome is exactly one UTF-8 JSON object plus a newline on stdout, with the keys `schema_version`, `command`, `status` (`ok` or `error`), `data`, `error`, `warnings` and `meta`; every key is always present. Diagnostics and `--verbose` output go to stderr and never contain relay URLs, arguments or secrets. No prompt is shown and stdin is never read.
- **Encodings.** Amounts and the event Unix time are canonical signed 64-bit decimal strings, never JSON numbers (`"0"` for sats means market price; fiat amounts are in the currency's advertised units, not cents). `created_at` is UTC RFC3339 with `Z`, or `null` outside years 0000-9999 (with one `invalid_event_timestamp` warning); `created_at_unix` keeps the exact integer. Range endpoints are `null` when absent. `created_at` is the selected event's time, not the order's creation time.
- **Listing semantics.** Public events of kind 38383 from the configured node, last seven days, limit 50 per relay, no upper time bound. The latest state of each order is chosen before the `--side`, `--currency` and `--status` filters (status defaults to `pending`; side and status are case-insensitive, currency is uppercased). Orders are sorted by event time, newest first. The snapshot in `meta` is a bounded read, never a complete history (`complete_history` is always `false`).
- **Limits.** Every configured relay must reach EOSE: 10 s to connect, 15 s to read, 1 s of cleanup, at most 10,000 unique events. A closed, silent or disconnected relay, a lagging buffer or a larger result fails the whole read; nothing is retried and no partial rows are returned. An empty book is a success with `orders: []`.
- **Errors.** Codes are stable, messages are fixed prose, `data` is `null` on every error, and `status` is never `uncertain` for these commands.

  | Code | Exit | When |
  | --- | --- | --- |
  | `invalid_arguments` | 2 | unknown, missing or invalid argument or side/status, removed spelling |
  | `output_not_supported` | 2 | `--output json` on a command without JSON support |
  | `configuration_missing` | 1 | `MOSTRO_PUBKEY` or `RELAYS` missing or blank |
  | `configuration_invalid` | 1 | invalid public key or relay URL |
  | `relay_connection_failed` | 1 | a relay could not be connected in time |
  | `relay_read_failed` | 1 | REQ rejected or CLOSED, disconnect before EOSE, lagging buffer |
  | `read_timeout` | 1 | the read deadline passed before every EOSE |
  | `read_limit_exceeded` | 1 | more than 10,000 unique events |
  | `internal_error` | 1 | unexpected local failure |

- **Exceptions to the single document.** `--help` and `--version` print text and exit 0 even with `--output json`. A missing, invalid or repeated `--output` exits 2 with empty stdout and a fixed stderr line. If the result cannot be written (closed or broken stdout) the process exits 1 with one fixed stderr line and nothing more. A crash or external kill cannot guarantee a final document.
- **Versioning.** `schema_version` (currently `1`) is independent of the package version, which `schema` reports separately as `executable_version`. Consumers may ignore additional optional fields; changed types, semantics, required fields or enum values need a new `schema_version`. The canonical command names are part of version 1. Because `executable_version` is in the generated reference, a package version change requires regenerating it.
- **Not supported by this contract**: pagination, an all-statuses filter, streaming output, and any write command. Secret-export commands (`getlasttradeprivkey`, `conversationkey`) are not offered in JSON.

### Orders

- **`order list`** (replaces `listorders`)
  - **Description**: Reads the node's public order book (read-only; publishes nothing and never touches the local database or identity). Needs `MOSTRO_PUBKEY` and `RELAYS`, from the root flags or the environment.
  - **Args**:
    - `--side <buy|sell>`: Optional order side (case-insensitive). Replaces `--kind`.
    - `--currency <STRING>`: Optional fiat currency code (case-insensitive).
    - `--status <STRING>`: Optional status (case-insensitive); defaults to `pending`. One of the canonical statuses listed in `mostro-cli order list --help`.
  - **Output**: the familiar table by default; one JSON document with `--output json` (see Machine output above).
  - **Migration**: `listorders`, `listorders --kind` and the short flags `-k`, `-c` and `-s` no longer exist (there are no aliases); use `order list --side --currency --status`. The root flag is `--mostro-pubkey`, still `-m` in short form, placed before the command word.
  - **Handler**: `commands::order::list::list_orders` through `msg::fetch_msg`; rendering in `parse_human` and `parse_machine`.

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
  - **Confirmation**: `y` (case-insensitive) or a real blank Enter approves the preview. Closed stdin/EOF is not approval. Rejection (`n`) and unrecognized answers return an error before publication; the CLI reports the error on stderr and exits 1.
  - **Market fiat**: With a zero sats amount, a currency absent from Yadio's list returns an error with an explicit-amount (`-a`) hint, rather than exiting successfully. A nonzero amount retains the existing fixed-rate bypass.

- **`takesell`**
  - **Description**: Take a sell order from a Mostro pubkey.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
    - `--invoice <STRING>`: Optional Lightning invoice.
    - `--amount <u32>`: Fiat amount to pay (someone is selling sats, you are buying).
  - **Handler**: `execute_take_order(order_id, Action::TakeSell, invoice, amount, ctx)` in `src/cli/take_order.rs`.

- **`takebuy`**
  - **Description**: Take a buy order from a Mostro pubkey.
  - **Args**:
    - `--order-id <UUID>`: Order identifier.
    - `--amount <u32>`: Fiat amount to receive (someone is buying sats, you are selling).
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

- **`order info`** (replaces `ordersinfo`)
  - **Description**: Asks the node for its record of your own orders. It publishes one signed request using your identity key, waits for the reply to that request, and writes nothing locally. It needs an existing local identity (`mcli.db`) and never creates one. Orders that are not yours or unknown to the node are not returned; they are listed as not returned.
  - **Args**: `--order <UUID>`, required, repeat it for several orders (no limit in the CLI; the node refuses what it will not answer). A repeated id is requested once.
  - **Output**: readable details by default, one JSON outcome with `--output json`. A node refusal is exit 1 with the node's reason, for example `too_many_requests`; the answer "none of these is yours" is a success with every id listed as not returned.
  - **Migration**: `ordersinfo -o ID` becomes `order info --order ID`; the short flag and `--orderids` are gone, and no alias remains. The existing MCP server's `get_order` tool runs the old command and stops working until it is migrated.
  - **Handler**: `commands::order::info::order_info` through `msg::exchange`; rendering in `parse_human::order_info` and `parse_machine`. The full contract is in `specs/002-order-info-json/contracts/cli.md`.

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
  - **Description**: Read both sides of the kind-14 chat for an order. The pubkey is the counterparty the conversation is derived from.
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
  - **Description**: Send a kind-14 peer chat message (`K_conv` / `K_sign`). Derives chat keys from the order’s trade keys and the recipient pubkey.
  - **Args**:
    - `--pubkey <NPUB/HEX>`: Recipient pubkey.
    - `--order-id <UUID>`: Order id to derive trade keys and shared key.
    - `--message <STRING>...`: Message parts; joined with spaces.
  - **Handler**: `execute_dm_to_user(PublicKey::from_str(pubkey)?, &ctx.client, order_id, &msg, &ctx.pool)` in `src/cli/dm_to_user.rs`.

- **`disputechat`**
  - **Description**: Read, and optionally send on, the dispute conversation with the solver. Uses the same kind-14 envelope as peer chat, derived from the order's trade keys and the stored solver pubkey.
  - **Args**:
    - `--order-id <UUID>`: Order that is in dispute.
    - `--since <i64>`: Minutes back from now to query (default: 1440).
    - `--message <STRING>`: Optional message to send to the solver before listing.
  - **Handler**: `execute_dispute_chat(order_id, since, message, ctx)` in `src/cli/dispute_chat.rs`.

### Direct messages (admin / solver)

- **`getadmindm`**
  - **Description**: Get the latest direct messages for admin.
  - **Args**:
    - `--since <i64>`: Minutes back from now to query (default: 30).
    - `--from-user`: If true, get messages from the counterparty instead of Mostro.
  - **Handler**: `execute_get_dm(since, true, from_user, ctx)` in `src/cli/get_dm.rs`.

- **`admsenddm`** *(admin only)*
  - **Description**: Send a direct message to a user as admin/solver.
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

- **`getuserinfo`**
  - **Description**: Get the reputation (rating, reviews, first trade date) the Mostro node holds for **your** identity key. Identity comes from the local wallet (`~/.mcli/mcli.db`); there is no user-npub argument — only Mostro’s pubkey (`-m` / `MOSTRO_PUBKEY`) is required. Sent from a trade key with the identity proof (reputation mode; avoid `-s` / `SECRET` if you want a real rating).
  - **First-contact PoW**: the trade key is not in an active order/dispute, so Mostro’s `pow_first_contact` lane applies. `ensure_first_contact_pow` fails fast with `PowRequirementUnmet` when configured `POW` is below the advertised difficulty; otherwise pass `--pow <pow_first_contact>` (or set `POW`).
  - **Reply correlation**: waits via `wait_for_dm_matching` until a decryptable `UserInfo` or `CantDo` echoes this request’s `request_id` (skips unrelated Mostro DMs to the same trade key). Does **not** use `validate_response` for dispatch — that helper returns `MostroCantDo` before checking the id.
  - **Args**: None.
  - **Handler**: `execute_user_info(ctx)` in `src/cli/user_info.rs`.

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

- **`admcancelpending`** *(operator only, admin gRPC)*
  - **Description**: Cancel a still-`pending` / `waiting-taker-bond` order through the daemon's `CancelOrder` RPC with `pretrade_only` set (any other status, a dispute included, is refused); maker notified, bonds released. Needs `MOSTRO_RPC_URL`, plus `MOSTRO_RPC_TOKEN` only when the daemon configures `[rpc].auth_token`; not `ADMIN_NSEC`.
  - **Requires**: `mostrod` ≥ 0.18.7 (the first release enforcing `pretrade_only`); the CLI calls `GetVersion` first and refuses older daemons.
  - **Args**:
    - `-o, --orderid <UUID>`: Order identifier.
  - **Handler**: `execute_cancel_pending(order_id)` in `src/cli/maintenance.rs`.

- **`admsettle`** *(admin only)*
  - **Description**: Settle a dispute.
  - **Args**:
    - `--dispute-id <UUID>`: Dispute identifier.
  - **Handler**: `execute_admin_settle_dispute(order_id, ctx)` in `src/cli/take_dispute.rs`.

- **`admaddsolver`** *(admin only)*
  - **Description**: Add a new dispute solver.
  - **Args**:
    - `--npubkey <PUBKEY>[:<CATEGORY>]`: Solver pubkey as npub or hex (hex is converted to npub before sending). Optional category: `read`, `read-write` or `write` (default: read-write).
  - **Requirements**: `ADMIN_NSEC` must be the Mostro daemon's own key; the CLI fails before sending otherwise.
  - **Behavior**: Waits for Mostro's `AdminAddSolver` confirmation. If Mostro replies `CantDo` or does not reply (e.g. the daemon rejected the request), the command fails and reports that the solver was not added.
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

1. Add the subcommand and its arguments in `src/cli/commands.rs`.
2. Add its handler function in an appropriate `src/cli/*.rs` module.
3. Extend the `Commands::run` match in `src/cli/legacy.rs` to call the handler.
4. Update this `commands.md` file so documentation stays in sync for humans and AI tools.

