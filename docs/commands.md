## CLI Commands Reference

This document lists all `mostro-cli` subcommands defined in `src/cli/commands.rs`, their main arguments, and the Rust handler that implements each command. Use this when wiring new commands or changing existing flows.

All commands are part of the `Commands` enum and are dispatched via `Commands::run(&self, ctx: &Context)`.

### Global options

These flags apply to every subcommand (parsed on the top-level `Cli`; each also
settable via the matching env var):

- `-m, --mostro-pubkey <NPUB|HEX>` (`MOSTRO_PUBKEY`): the Mostro node to talk to. Required for every command except `schema`, `identity init`, `identity show` and `order mine`. The old spelling `--mostropubkey` was removed without an alias; scripts must switch.
- `-r, --relays <CSV>` (`RELAYS`): comma-separated relay URLs.
- `-p, --pow <BITS>` (`POW`): the least NIP-13 proof of work mined on outgoing events, default 0. The commands that go
  through `msg::exchange` (the JSON-capable writes and `order info`) raise it to what the node advertises for the
  request's lane, up to 20 bits; above that they fail with `pow_required` carrying `required` and `configured` (spec 017).
  The legacy commands mine exactly this value. `order chat` and `order watch` mine nothing and do not read it.
- `-s, --secret` (`SECRET=true`): full-privacy mode (unsigned inner tuple; identity = trade key).
- `-t, --transport <nip44>` (`TRANSPORT`): wire transport. Only `nip44`
  (protocol v2, signed kind 14) is supported.
  **Optional**: when omitted, the CLI auto-detects it at startup from the node's
  `protocol_version` tag on its kind-38385 info event (a node that advertises
  nothing is treated as `nip44`). A v1 node is an error. `order chat` and `order watch` do not read it: they talk to relays, never
  to the node.
- `-v, --verbose`: enable info-level logging (also surfaces the resolved transport).
- `--reply-timeout <SECONDS>` (spec 016): how long to wait for the node's reply, 5 to 300 seconds, default 15; accepted before, between or
  after the command words, like `--output`. It is the deadline of the commands that wait for a reply: `order new`, `order take-sell`,
  `order take-buy`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order info`, `dispute open`, `order rate` and
  `bond add-invoice`. The connect, probe and cleanup budgets do not change; the public reads (`order list`, `message list`, `order sync`, `order
  watch`) keep their own limits, and the legacy commands ignore it. A value out of range or not a number is `invalid_arguments` (exit 2). A reply after the
  deadline is still `reply_timeout` (`uncertain` for a write), never retried: raise it for a slow node rather than repeating a write.
- `--output <human|json>`: output format, accepted before, between or after
  command words (`--output json` and `--output=json`; case-sensitive, default
  `human` whatever the terminal, no environment variable). Only `order list`,
  `schema`, `order info`, `order new`, `order take-sell`, `order take-buy`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order sync`, `order mine`, `order rate`, `dispute open`, `order chat`, `order watch`, `bond add-invoice`, `message list`, `identity init` and `identity show` support `json`; on any other command it prints one JSON error outcome
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
  - **Generated reference**: [docs/schemas/discovery-v1.json](schemas/discovery-v1.json) is this command's output, and [docs/schemas/order-list-v1.json](schemas/order-list-v1.json) the `order.list` output schema; every machine command has its own `docs/schemas/<noun>-<verb>-v1.json`. All are generated and drift-tested; this prose is maintained by hand. Regenerate them after an intended change with `cargo run -q -- schema > docs/schemas/discovery-v1.json` and, per command, `cargo run -q -- --output json schema | jq '.data.commands[] | select(.name=="order.list") | .output_schema' > docs/schemas/order-list-v1.json` (the drift test prints the full list).
  - **Global options**: `global_options` (in the raw document and in `data` of the JSON outcome) lists the options accepted at any command
    depth, generated from the grammar: `name`, `type`, `description`, `enum` and `default` when they exist, and `applies_to`, the canonical
    names the option takes effect on. Today: `output` (every machine command) and `reply-timeout` (default `"15"`, the eleven commands that wait
    for the node's reply). They are not part of any command's input schema.
  - **Input schemas** mark an option whose value `-` reads it from standard input with `"x-stdin": "-"`; today that is `invoice` on `order new`, `order take-sell`, `order add-invoice` and `bond add-invoice` (spec 010).

### Machine output (schema v1)

This applies to `order list`, `schema`, `order info`, `order new`, `order take-sell`, `order take-buy`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order sync`, `order mine`, `order rate`, `dispute open`, `order chat`, `order watch`, `bond add-invoice`, `message list`, `identity init` and `identity show`. Other commands print human text only. The errors below are those of the listing; `order info`, `order new`, the takes, the invoice commands and the identity commands add their own codes, listed in their contracts ([info](../specs/002-order-info-json/contracts/cli.md#stable-errors), [new](../specs/003-order-new-json/contracts/cli.md#stable-errors), [take](../specs/004-order-take-json/contracts/cli.md#stable-errors), [invoice](../specs/005-invoice-json/contracts/cli.md#stable-errors), [identity](../specs/009-identity-json/contracts/cli.md#stable-errors)).

- **One document.** With `--output json`, a controlled outcome is exactly one UTF-8 JSON object plus a newline on stdout, with the keys `schema_version`, `command`, `status` (`ok` or `error`), `data`, `error`, `warnings` and `meta`; every key is always present. Diagnostics and `--verbose` output go to stderr and never contain relay URLs, arguments or secrets. No prompt is shown and stdin is never read for an answer; the only read of stdin is the invoice of `--invoice -` (spec 010), which requires `--yes`.
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

- **The invoice reason.** On `order new`, `order take-sell`, `order add-invoice` and `bond add-invoice`, an `invalid_arguments` caused by the invoice value itself carries `"reason": "invalid_invoice"` in `error`, as a node refusal carries its reason; the message stays the fixed text and never contains the invoice. It covers what the program checks before sending: for `order add-invoice` a BOLT-11 prefix (`lnbc`, `lntb`, `lnsb`, any case) or a Lightning address with one `@`, for `bond add-invoice` a BOLT-11 prefix only (an address is refused), for `order take-sell` and `order new` a non-empty value of at most 4096 bytes without whitespace or control characters, and for `--invoice -` the content of standard input (empty, several lines, more than 8 KiB, not UTF-8). Every other `invalid_arguments` carries no reason: a malformed order id, a missing or unknown option, `--invoice -` without `--yes` or with a terminal as stdin. The program does not decode a BOLT-11 or check its network; the node refuses such an invoice (`node_refused`, reason `invalid_invoice`). The generated schemas allow a reason only on `node_refused` and, for those four commands, `invalid_invoice` on `invalid_arguments` (2026-11-05).
- **Exceptions to the single document.** `--help` and `--version` print text and exit 0 even with `--output json`. A missing, invalid or repeated `--output` exits 2 with empty stdout and a fixed stderr line. If the result cannot be written (closed or broken stdout) the process exits 1 with one fixed stderr line and nothing more. A crash or external kill cannot guarantee a final document.
- **Versioning.** `schema_version` (currently `1`) is independent of the package version, which `schema` reports separately as `executable_version`. Consumers may ignore additional optional fields; changed types, semantics, required fields or enum values need a new `schema_version`. The canonical command names are part of version 1. Because `executable_version` is in the generated reference, a package version change requires regenerating it.
- **Not supported by this contract**: pagination, an all-statuses filter, streaming output (human `order watch` is the only output that
  streams, constitution 1.0.1), and any write command. Secret-export commands (`getlasttradeprivkey`, `conversationkey`) are not offered in JSON.

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

- **`order new`** (replaces `neworder`)
  - **Description**: Create a buy or sell order. Needs an existing local identity (it never creates one), publishes exactly one signed request with a fresh trade key and saves the order the node creates.
  - **Args**:
    - `--side <buy|sell>`, `--currency <CODE>`, `--fiat-amount <N>`, `--payment-method <TEXT>` (required).
    - `--fiat-amount-max <M>`: makes a range order `N..M` at market price (`--sats` must then be 0).
    - `--sats <N>` (default 0, market price), `--premium <PERCENT>` (-100..100, default 0), `--invoice <BOLT11>` (`-` reads it from standard input; see below), `--expiration-days <N>` (0..365; 0 uses the node's default).
    - `--expires-in <SECONDS>` (1..31536000; spec 012): the order asks for `expires_at = now + SECONDS` instead of whole days. mostrod uses it as given and only lowers it to its `max_expiration_days`, so a short value is legal. It conflicts with `--expiration-days` (`invalid_arguments`, exit 2, before anything happens). Discovery lists it as the input property `expires-in` of `order.new`.
    - `--yes`: create without asking. Required with `--output json`, which never prompts or reads stdin for an answer.
  - **Output**: the order preview and a prompt (blank or `y` accepts; `n`, anything else or closed stdin declines) in human mode; one JSON document with `--output json`. Statuses are `ok`, `error` and `uncertain`: `uncertain` means the node may hold the order (no reply, an unusable reply, or the order could not be saved locally); check `order list` before trying again. The trade index is recorded right before the request is published, so a key is never reused, even when the run dies before the reply, and two runs at once never share one; a refusal or a request that never left gives it back. A refusal for a stale index (`invalid_trade_index`) keeps it instead (`meta.persistence` `applied`): the node is already at or past it, so the next run carries the one after it. The node can record an index and still refuse (a take that lost the order to another taker, the first request of a new identity); the next request is then refused once as stale and the one after it goes through. The preview shows the index the order will use unless another command reserves one meanwhile; the result reports the one actually used. Two requests at once may reach the node out of order, and the node then refuses the lower index with `invalid_trade_index`. On a node that requires a maker bond the result is `ok` with `data.next_step` `pay_bond_invoice` and the bond invoice in `data.payment_request`: pay it with any Lightning wallet and the node then publishes the order.
  - **Migration**: `neworder`, its short flags, the squashed long flags (`--fiatcode`, `--paymentmethod`, `--expirationdays`), `--kind`, `--amount` and the `100-200` range syntax are gone with no alias; use `--side`, `--sats`, `--fiat-amount` with `--fiat-amount-max`. The Yadio currency check is gone: the node validates. The existing MCP server's `create_order` tool runs the old command and stops working until it is migrated.
  - **Handler**: `commands::order::new::order_new` through `msg::exchange` and `store`; dispatch and the prompt in `cli/order_new.rs`.

- **`order take-sell`** and **`order take-buy`** (replace `takesell` and `takebuy`)
  - **Description**: Take an order from the book. The side names the order taken: `take-sell` takes a sell order (you buy sats), `take-buy` takes a buy order (you sell sats). Needs an existing local identity (it never creates one), publishes exactly one signed request with a fresh trade key and saves the trade.
  - **Args**:
    - `--order <UUID>` (required).
    - `--amount <FIAT>`: the fiat amount, for a range order.
    - `--invoice <BOLT11|ADDRESS>`: take-sell only; without one the node asks for it next. `-` reads it from standard input (see `order add-invoice`).
    - `--yes`: take without asking. Required with `--output json`, which never prompts or reads stdin for an answer.
  - **Output**: a preview and a prompt in human mode (blank or `y` accepts; anything else and closed stdin decline); one JSON document with `--output json`. The data names the next step (`add_invoice`, `wait_for_seller_payment`, `pay_hold_invoice`, `pay_bond_invoice`) and carries the hold invoice or the taker bond invoice to pay when there is one; pay it with any Lightning wallet, the node continues afterwards. Statuses are `ok`, `error` and `uncertain` as for `order new`: `uncertain` means the node may hold the take; check `order info --order <id>` before trying again. The trade index follows `order new`: recorded before the take is published, given back after a refusal or a take that never left, and kept after a refusal for a stale index (`invalid_trade_index`).
  - **Migration**: `takesell`, `takebuy`, `-o`, `--orderid`, `-i` and `-a` are gone with no alias; use `order take-sell|take-buy --order ... --invoice ... --amount ...`. An invalid invoice is now refused before anything is sent. The legacy read of the public book after a take-sell with an invoice is gone: the saved row is minimal (id, side, `waiting-payment`, trade key) until a later command refreshes it. The existing MCP server's `take_order` tool runs the old commands and stops working until it is migrated.
  - **Handler**: `commands::order::take::order_take` through `msg::exchange` and `store`; dispatch and the prompt in `cli/order_take.rs` (shared steps in `cli/write.rs`).

- **`order add-invoice`** (replaces `addinvoice`)
  - **Description**: Send the invoice the node asked for on an order you hold (after `order take-sell` without an invoice, or when the node asks again). Needs an existing local identity (it never creates one) and the order in the local store; publishes exactly one signed request with that order's stored trade key and **spends no trade index**. After an accepted reply the local status follows the node's.
  - **Args**:
    - `--order <UUID>`, `--invoice <BOLT11|ADDRESS>` (required).
    - `--invoice -` reads the invoice from standard input instead of the command line (spec 010, also on `bond add-invoice`, `order take-sell` and `order new`): at most 8 KiB of UTF-8 holding exactly one non-empty line once ASCII whitespace is trimmed, read until end of input, so **the writer must close standard input after the line** (a pipe or a file does; a second line is refused, not ignored). Empty input, a second line, more than 8 KiB or invalid UTF-8 is `invalid_arguments` (exit 2) before anything happens. The line is then validated like a literal value. Because stdin carries the invoice, this form needs `--yes` in human mode too; without it, or when stdin is a terminal (which never ends the input by itself), the command fails at once with `invalid_arguments` without reading stdin. The invoice never appears in a preview, diagnostic or JSON document.
    - `--yes`: send without asking. Required with `--output json`, which never prompts or reads stdin for an answer.
  - **Output**: a preview (never the invoice) and a prompt in human mode; one JSON document with `--output json`. `data.result` is `waiting_seller_payment`, `payment_accepted` (with the order) or `invoice_updated`. A failed local status update is the warning `local_status_stale`; the node refusing is `node_refused` with its reason; no or an unusable reply is `uncertain`. An order not in the local store is `order_unknown` and nothing is sent.
  - **Migration**: `addinvoice`, `-o`, `--order-id` and `-i` are gone with no alias; use `order add-invoice --order ID --invoice INVOICE`. The existing MCP server has no tool for it.
  - **Handler**: `commands::order::invoice::order_add_invoice` through `msg::exchange` and `store`; dispatch and the prompt in `cli/invoice.rs`.

- **`bond add-invoice`** (replaces `addbondinvoice`)
  - **Description**: Reply to the node's bond payout request with a BOLT-11 invoice for exactly your share of a slashed bond. This is unrelated to `pay-bond-invoice`, the bond you pay when an order or a take asks for one. Writes nothing locally.
  - **Args**: as `order add-invoice` (including `--invoice -`), but the invoice must be BOLT-11 (an address is `invalid_arguments`).
  - **Output**: `data.result` is `invoice_accepted` with the share the node reported. Nodes before v0.20.1 never confirm: silence is `uncertain` (`reply_timeout`), not success, because the program cannot tell it from a lost reply. The confirmation carries no request id and is matched on the order.
  - **Migration**: `addbondinvoice` and its short flags are gone with no alias; use `bond add-invoice --order ID --invoice BOLT11`. The old "timeout means success" reading is gone. The existing MCP server has no tool for it.
  - **Handler**: `commands::order::invoice::bond_add_invoice`; dispatch in `cli/invoice.rs`.

- **`order fiat-sent`** (replaces `fiatsent`)
  - **Description**: Tell the node that you, the buyer, sent the fiat. Publishes one signed request with the stored trade key of the order and updates the local status to `fiat-sent`. It needs an existing local identity (`mcli.db`) and never creates one. In JSON mode it requires `--yes`.
  - **Args**: `--order <UUID>`, required; `--yes`.
  - **Output**: `data.result` is `fiat_sent` and `data.next_step` is `wait_for_release`. A refusal (for example `invalid_pubkey` for the seller, `not_allowed_by_status`) is `node_refused` with the reason; silence or an unusable reply is `uncertain` and is not retried. A failed local status update is a success with the warning `local_status_stale`. On a range order you made with room left, a buyer's `fiat-sent` carries a fresh trade key (the node keeps it until the seller releases) and `data.next_trade_index` reports the index reserved for it; the child order appears when the seller releases, and `order sync` gives it a local row. The index is recorded before the request is published, so a run that dies before the reply never lets a later order reuse the key; a refusal gives it back, and an index that cannot be reserved is `store_unwritable` with nothing sent.
  - **Migration**: `fiatsent`, `-o` and `--order-id` are gone with no alias. The existing MCP server has no tool for it.
  - **Handler**: `commands::order::step::order_step` through `msg::exchange` and `store`; dispatch and the prompt in `cli/step.rs`.

- **`order release`** (replaces `release`)
  - **Description**: Settle the hold invoice so the node pays the buyer. Irreversible; the preview says so. Same rules, arguments and failure behaviour as `order fiat-sent`; the local status becomes `settled-hold-invoice`. On a range order you made as the seller, with room left, it carries a fresh trade key so the node creates the child order for the rest of the range (see `order sync`).
  - **Output**: `data.result` is `released`; with a key sent, `data.next_trade_index` is the reserved index and `data.next_step` is `sync_child_order`. The `rate` message the node sends afterwards is visible with `message list`.
  - **Migration**: `release`, `-o` and `--order-id` are gone with no alias; the existing MCP server has no tool for it.

- **`order cancel`** (replaces `cancel`)
  - **Description**: Cancel an order. The node decides what the request means from the order status, and `data.result` tells which: `canceled` (a pending or waiting order, canceled at once), `cancel_requested` (a cooperative cancel was started on an active trade; `next_step` is `wait_for_peer_cancel` and the other party confirms with `order cancel`) or `cancel_accepted` (you confirmed the other party's request). Same rules, arguments and failure behaviour as `order fiat-sent`; the local status is `canceled` or `cooperatively-canceled`, and unchanged for a request.
  - **Migration**: `cancel`, `-o` and `--order-id` are gone with no alias; the existing MCP server has no tool for it.

- **`order sync`** (new)
  - **Description**: Give the orders the node announced to your trade keys a local row. A range order's child order (see `order release` and `order fiat-sent`) is created by the node for a fresh trade key; the node announces it with a `new-order` message, and the commands that act on an order need its row. `order sync` reads those messages like `message list` (one bounded read, the node as the author, your derived trade keys as recipients), saves a row for every announced order the store does not know, with the key of its index and as your own order, and publishes nothing. It never replaces a row, so a second run saves nothing. No `--yes` and no prompt: nothing is published and only rows for your own keys are added. It needs an existing local identity and a writable store.
  - **Args**: `--since MINUTES` (1 to 10080, default 10080).
  - **Output**: `data.adopted` lists the orders added (the order, the trade index and key); `messages_read`, `messages_ignored`, `keys_read`, `keys_not_read` and `truncated` say what was looked at. When nothing is due the run is a success with an empty list and persistence `not_attempted`. A save that fails stops the run and is `uncertain` (`order_not_saved`) with the rows already saved; run it again. For a buy range order the child exists only after the seller releases, so run it then.
  - **Handler**: `commands::order::sync::order_sync` through `msg::inbox` and `store`; dispatch in `cli/sync.rs`.

- **`order mine`** (new; spec 011)
  - **Description**: The orders of the local store (made, taken, or adopted by `order sync`, range children included), newest first, read-only. No node, relay or Mostro key.
  - **Args**: none.
  - **Output**: `data.orders[]` with `id`, `side`, `status` (the local status), `trade_index` (decimal string) and `trade_pubkey` (hex). The store keeps a row's trade key, not its index, so the index is found by deriving the newest 200 recorded indexes (as `message list` does); a key older than those, or a row without a key, has `trade_index: null` (and `trade_pubkey: null` without a key), with the warning `trade_keys_limited` when older keys were not derived (in human mode, a warning line on stderr). Newest first means the highest index first, then the rows whose index is unknown by creation time. No store or user is `identity_missing`; a store or a row that cannot be read is `store_unreadable`.
  - **Handler**: `commands::order::mine::order_mine` through `Store::order_rows` (`store/rows.rs`) and `Store::trade_keys`; dispatch in `cli/mine.rs`.

- **Bonds** (spec 015, verified against the scripted node): after a bonded `order take-*` or `order new` (`next_step` `pay_bond_invoice`), the local row holds the order the node sent (status `pending`) and the trade key, so `order cancel --order <id>` works for a taker whose take waits on its bond (the node releases the bond and puts the order back on the book) and for a maker whose order waits on its bond (never published; closed). Both answer `data.result` `canceled`, local status `canceled`. A maker bond that locked meanwhile is refused with `not_allowed_by_status` (`node_refused`); this comes from the mostrod v0.20.2 source (`cancel_waiting_maker_bond_order`) only: the scripted node does not produce it and no real node was probed. `message list --order <id>` reports `add-bond-invoice` (`next_step` `add_bond_invoice`, payload `bond_payout_request` with the share in `payload.order.sats_amount` and `slashed_at`) and `bond-invoice-accepted`, `bond-payout-completed` and `bond-slashed` with an `order` payload (the share, or the slashed amount; `bond-slashed` has no status).

- **`order info`** (replaces `ordersinfo`)
  - **Description**: Asks the node for its record of your own orders. It publishes one signed request using your identity key, waits for the reply to that request, and writes nothing locally. It needs an existing local identity (`mcli.db`) and never creates one. Orders that are not yours or unknown to the node are not returned; they are listed as not returned.
  - **Args**: `--order <UUID>`, required, repeat it for several orders (no limit in the CLI; the node refuses what it will not answer). A repeated id is requested once.
  - **Output**: readable details by default, one JSON outcome with `--output json`. A node refusal is exit 1 with the node's reason, for example `too_many_requests`; the answer "none of these is yours" is a success with every id listed as not returned.
  - **Migration**: `ordersinfo -o ID` becomes `order info --order ID`; the short flag and `--orderids` are gone, and no alias remains. The existing MCP server's `get_order` tool runs the old command and stops working until it is migrated.
  - **Handler**: `commands::order::info::order_info` through `msg::exchange`; rendering in `parse_human::order_info` and `parse_machine`. The full contract is in `specs/002-order-info-json/contracts/cli.md`.

### Disputes

- **`dispute open`** (replaces `dispute`; spec 013)
  - **Description**: Open a dispute on an active or `fiat-sent` order you hold. Publishes one signed request (`dispute`, no payload) with the order's stored trade key and spends no trade index. The node answers `dispute-initiated-by-you` with the dispute id; the local status becomes `dispute`. Supports `--output json` (`dispute.open`), which requires `--yes`.
  - **Args**: `--order <UUID>` (required), `--yes`.
  - **Output**: `data.order_id`, `data.dispute_id`, `data.trade_pubkey`, `data.node_pubkey`. A refusal is `node_refused` with the node's reason (`not_allowed_by_status` for an order that is not active or fiat-sent, `invalid_pubkey` for a key that is neither party). The node answers a second dispute on the same order with **silence**, so that run is `uncertain` (`reply_timeout`); nothing is retried. A failed local status update is the warning `local_status_stale`.
  - **Migration**: `dispute -o|--orderid <UUID>` (older documents wrote `--order-id`) is gone with no alias; use `dispute open --order <UUID> [--yes]`.
  - **Handler**: `commands::order::dispute::dispute_open` through `commands::order::held` (the request on a held order shared with `order rate`); dispatch in `cli/held.rs`.

- **`listdisputes`**
  - **Description**: Request open disputes from the Mostro pubkey.
  - **Args**: None.
  - **Handler**: `execute_list_disputes(ctx)` in `src/cli/list_disputes.rs`.

### Direct messages (user)

- **`message list`** (replaces `getdm`)
  - **Description**: What the node sent to your trade keys, oldest first: the next step of each trade (`next_step`) and any invoice to pay as data. Read-only: it writes nothing, saves no counterparty or solver key, and needs an existing identity (it never creates one). Supports `--output json` (`message.list`).
  - **Args**:
    - `--since <minutes>`: 1 to 10080 (default 30); anything else is `invalid_arguments`.
    - `--order <UUID>`: read only the messages of that order (`order_unknown` if the database has no key for it).
  - **Handler**: `message_list` in `src/commands/message/list.rs`; dispatch in `src/cli/message.rs`; contract in [specs/006-message-list-json](../specs/006-message-list-json/contracts/cli.md).
  - **Migration**: `getdm`, `-s` and `--from-user` are gone with no alias. Peer chat is `order chat` (`getdmuser -p` when it answers `counterparty_unknown` and you know the key); the solver's key is read from the `peer` payload of `admin-took-dispute`. The existing MCP server has no DM tool.

- **`order chat`** (spec 019; replaces `getdmuser` and `dmtouser`)
  - **Description**: Reads the peer chat (kind 14) of an order you hold and, with `--message`, sends one message to its counterparty; you
    never pass the counterparty's key. Needs `MOSTRO_PUBKEY`, `RELAYS`, an existing identity (it never creates one) and the order's row.
    Supports `--output json` (`order.chat`), where `--message` requires `--yes`.
  - **Args**:
    - `--order <UUID>`: an order in the local store; otherwise `order_unknown`, before any relay is contacted.
    - `--message <TEXT>`: one value, trimmed, 1 to 4096 bytes; a second `--message` is an argument error; write `--message=<TEXT>` for a
      text that starts with `-`. Human mode sends it without a prompt, as `dmtouser` does.
    - `--since <minutes>`: 1 to 10080 (default 10080, the seven-day chat window).
    - `--yes`: required with `--message` in JSON (otherwise `invalid_arguments`, exit 2, before configuration, store or relays);
      human mode never asks; without `--message` it does nothing.
    - Any invalid value exits 2 before any relay is contacted, and the message text is never shown.
  - **Counterparty**: the order row's `counterparty_pubkey`, when it parses and is not your own trade key, is used as it is, with no relay
    contacted. The legacy `getdm` may have stored it (from the first node message naming both trade keys) and never refreshes it.
    Otherwise all the node's messages to the order's trade key are read once, and only the newest `buyer-took-order` or
    `hold-invoice-payment-accepted` naming both trade keys, or `fiat-sent-ok` with its peer, names it; the key is then cached in the row (a
    failed save is not reported). Neither is `counterparty_unknown` (exit 1): the counterparty is known once the escrow is locked. A trade
    whose lock-time messages are gone (beyond the newest 50 per relay, or expired after the node's `dm_days`) gets it too; `getdmuser -p`
    and `dmtouser -p` stay the fallback when you know the key.
  - **Behavior**: the read comes before the send, so a failed read sends nothing. One bounded read of both sides in the window, oldest
    first, at most 200, opened only with your trade key and the counterparty's as writers. A message no relay confirmed while one failed
    without a clear answer may have been delivered; it is never resent. `--pow`, `--transport` and `--reply-timeout` do not apply: the
    message goes to relays, never to the node, mines nothing and waits for no reply. Worst case 52 s for a read and 78 s with `--message`.
  - **Human output**: "💬 Order Chat", the order and the counterparty, then "📭 No chat messages" or one block per message (UTC time, "🫵
    You" or "👤 Counterparty", the text indented with control, format and line separator characters escaped); a line when older messages
    may be missing; "📤 Your message (id ...)" when one was sent. Errors are one `error: <fixed message>` line on stderr, never with the
    message text. A send that may have reached a relay adds "The message may have reached a relay. Run `order chat --order <id>` and look
    for your message before sending it again." and still prints the conversation and the message. `--verbose` adds one line,
    `verbose: order.chat <ok|code>`.
  - **Output** (JSON): `data.order_id`, `data.counterparty_pubkey`, `data.messages` (oldest first by time and id, at most 200, never
    the message sent on this run), `data.truncated` (a relay or the cap may have left out older messages) and `data.sent` (the message
    sent, non-null exactly when publication is `published` or `unknown`). Each record has `message_id` (the inner event id), `from` (`me`
    or `counterparty`), `created_at`, `created_at_unix` and `text` (untrusted, at most 4096 bytes). `meta.snapshot` is always null.
    Codes: `invalid_arguments`, the configuration, identity and store codes, `order_unknown`, `counterparty_unknown`, the relay read codes,
    `request_rejected` and `internal_error`; warnings `invalid_event_timestamp` and `cleanup_incomplete`. Output schema
    [docs/schemas/order-chat-v1.json](schemas/order-chat-v1.json); the input schema is in the discovery entry.
  - **Stages** (an amendment for this command): publication is `not_applicable` until the send step runs, with or
    without `--message` (arguments, configuration, identity, store, `order_unknown`, `counterparty_unknown`, a failed lookup or read). The
    send step reports `published`; `not_attempted` when the wrap fails (`internal_error`), the relays cannot be connected (the connect
    code) or every relay refused (`request_rejected`); `unknown` (`uncertain`, `request_rejected`) when no relay confirmed and one failed
    without a clear answer. Node acceptance and persistence are always `not_applicable`: the counterparty cache is not reported. Example
    outcomes: [contracts/examples.json](../specs/019-order-chat/contracts/examples.json).
  - **Handler**: `order_chat` in `src/commands/order/chat.rs`; dispatch in `src/cli/order_chat.rs`; contract in
    [specs/019-order-chat](../specs/019-order-chat/contracts/cli.md).

- **`order watch`** (spec 020)
  - **Description**: Follows an order you hold until its trade ends: the node's messages to the order's trade key and, once the
    counterparty is known, the order's peer chat. The window is replayed first, then new records arrive live. Read-only, publishes
    nothing; needs `MOSTRO_PUBKEY`, `RELAYS`, an existing identity (it never creates one) and the order's row. Supports `--output json`
    (`order.watch`): one document per call, so a program loops watch, act, watch with `--after`.
  - **Args**:
    - `--order <UUID>`: an order in the local store; otherwise `order_unknown`, before any relay is contacted.
    - `--since <minutes>`: how far back to replay, 1 to 10080 (default 30).
    - `--after <cursor>`: start where an earlier JSON call stopped, its `data.cursor` passed back verbatim; conflicts with `--since`.
    - `--timeout <seconds>`: 1 to 86400, counted from the end of the first replay. JSON defaults to 30; human mode has no deadline
      without it.
    - Any invalid value exits 2 before configuration, store or relay access, and is never shown.
  - **Counterparty**: found as `order chat` finds it (the row, else the node's post-lock messages, cached in the row). When it is not
    known yet the watch starts without the chat, and the first new node message that names it brings the chat in, with its replay; that
    key is not saved, the next call's lookup caches it.
  - **Behavior**: one subscription per relay for the node's messages and one for the chat; each record is shown once (node messages by
    event id, chat messages by their inner `message_id`), across relays and resends. A relay that drops reconnects on its own (every 10
    s) and replays the window, and a subscription a relay dropped is sent again when it reconnects; no relay failure ends a watch once
    it is live. The opening replay is strict, as the reads: every relay must answer within 10 s to connect and 15 s to replay. After
    10,000 records the watch ends with `read_limit_exceeded`.
  - **End**: a node message for this order with `purchase-completed`, `hold-invoice-payment-settled`, `canceled`,
    `cooperative-cancel-accepted` or `admin-canceled` ends the watch; `admin-settled` ends it only when the order's row says you are the
    seller (the buyer's payout follows, and can fail and ask for a new invoice). `rate`, `bond-slashed` and `add-bond-invoice` may follow
    an end; read them with `message list --order`. Some watches never end on their own: an untaken order that expires (the node sends the
    maker nothing) and a trade that ended before the window. Use `--timeout`, and `order info` to check the order's status.
  - **Human output**: "👀 Watching order <id>", the window start and the counterparty (or "not known yet; the chat is followed once the
    escrow is locked"); "📭 Nothing yet; waiting for the node or the counterparty..." when the replay is empty; then each update as it
    comes: `message list`'s message block with its next step under "📨 From Mostro:", and `order chat`'s block under "💬 Chat:" (the
    text escaped); a line when the counterparty is learned and one when older records may be missing. Each update is written whole and
    flushed. The last line is "🏁 The trade ended: <action>", or at `--timeout` "⏰ Stopped at the watch deadline; the trade has not
    ended."; both exit 0. Ctrl-C stops it, with no exit status promised. Errors are one `error: <fixed message>` line on stderr (exit 1,
    2 for arguments); a stdout that cannot be written stops the watch with "error: the watch could not be written" (exit 1).
    `--verbose` adds one line, `verbose: order.watch <ok|code>`.
  - **Output** (JSON): one document per call, at the first new records after the replays, or an empty success at the deadline. An empty
    result means nothing was observed (relays may have been down), not that nothing happened. `data.order_id`,
    `data.counterparty_pubkey` (null while the chat is not followed), `data.messages` (`message list`'s records, with `next_step`),
    `data.chat` (`order chat`'s records, each once by `message_id`), both oldest first; `data.ended` (one of the messages ended the trade,
    as above: stop there and run `order rate` without waiting for `rate`);
    `data.truncated` (a relay's replay reached its limit of 50, so older records may be missing); `data.cursor`. The cursor is opaque:
    pass it back verbatim. It covers only accepted records, by their envelope time, clamped to now and kept 60 s behind the newest
    (the chat clock skew), with the ids already seen at or after it (at most 200; beyond that its time rises over records more than
    60 s old). Limit: a record that reaches the relays after records stamped more than 60 s later, or one inside a raised range, is
    never returned by a later call. `meta.snapshot` is always null and every stage is `not_applicable`; the counterparty cache write
    is not reported. Codes: `invalid_arguments`, the configuration, identity and store codes, `order_unknown`, the relay read codes,
    `read_limit_exceeded` and `internal_error`; warnings `invalid_event_timestamp` and `cleanup_incomplete`. Output schema:
    [docs/schemas/order-watch-v1.json](schemas/order-watch-v1.json); the input schema is in the discovery entry.
    Example outcome: [contracts/examples.json](../specs/020-order-watch/contracts/examples.json).
  - **Timing**: 10 s to connect, 15 s for the replay, 15 s more when the counterparty is learned, then live until the end or
    `--timeout`, then 1 s of cleanup; up to 26 s more first when the counterparty must be looked up. `--pow`, `--transport` and
    `--reply-timeout` do not apply. A JSON call takes at most `--timeout` + 67 s (a reconnect during a replay does not extend it);
    allow `--timeout` + 70 s.
  - **Handler**: `order_watch` in `src/commands/order/watch.rs`; dispatch in `src/cli/order_watch.rs`; contract in
    [specs/020-order-watch](../specs/020-order-watch/contracts/cli.md).

- **`getdmuser`**
  - **Description**: Read both sides of the kind-14 chat for an order. The pubkey is the counterparty the conversation is derived from. Needs the order's row and its trade key, as `order chat` does; `order chat` also finds the counterparty itself, so this command is the fallback when it answers `counterparty_unknown` and you know the key.
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
  - **Description**: Send a kind-14 peer chat message (`K_conv` / `K_sign`). Derives chat keys from the order’s trade keys and the recipient pubkey, so it needs the order's row, as `order chat` does; `order chat --message` also finds the recipient itself, so this command is the fallback when it answers `counterparty_unknown` and you know the key.
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

- **`identity init`** (new; spec 009)
  - **Description**: Create the local identity store exactly as the first run of a legacy command does: the data directory (`MCLI_DIR`, else `~/.mcli`, owner-only), `mcli.db` (owner-only) with its two tables and one user with a fresh 12-word mnemonic and no trade index. It needs no node, relay, Mostro key, stdin or `--yes`, connects to nothing and never shows the mnemonic or a private key. If `mcli.db` (or a leftover `mcli.db-journal`, `-wal` or `-shm`) already exists it changes nothing and fails with `identity_exists` (exit 1). Supports `--output json` (`identity.init`).
  - **Args**: none.
  - **Output**: `data.identity_pubkey` (64 lowercase hex). Stages: publication and node acceptance `not_applicable`, persistence `applied` (or `not_attempted`/`failed` on an error). A directory or file that cannot be created is `store_unwritable`.
  - **Handler**: `commands::identity::identity_init` through `store::create_store` (`store/create.rs`, shared with `db::connect`); dispatch in `cli/identity.rs`; contract in [specs/009-identity-json](../specs/009-identity-json/contracts/cli.md).

- **`identity show`** (new; spec 009)
  - **Description**: Show the identity public key and the last trade index of the local store, read-only (nothing is created or changed). No node, relay or Mostro key. Supports `--output json` (`identity.show`).
  - **Args**: none.
  - **Output**: `data.identity_pubkey` and `data.last_trade_index` (a JSON integer, `0` when none is recorded). `identity_missing` without a store or user, `identity_unreadable` for a damaged one. The mnemonic is never shown; there is no `--reveal`.
  - **Handler**: `commands::identity::identity_show` through `identity::load_identity_from`; dispatch in `cli/identity.rs`.

- **`conversationkey`**
  - **Description**: Get the conversation key for direct messaging with a user.
  - **Args**:
    - `--pubkey <NPUB/HEX>`: Counterparty pubkey.
  - **Handler**: `execute_conversation_key(&ctx.trade_keys, PublicKey::from_str(pubkey)?)` in `src/cli/conversation_key.rs`.

- **`getlasttradeindex`**
  - **Description**: Get last trade index of the user and raise the local one to it. It never lowers the local index: the node does not count the keys of range children, so a lower answer is kept out and the local index is printed instead.
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

- **`order rate`** (replaces `rate`; spec 014)
  - **Description**: Rate the counterparty of a finished trade you hold (the order is `success`, or `settled-hold-invoice` when you are the seller). One signed request (`rate-user` with the rating) with the order's stored key; writes nothing locally. Supports `--output json` (`order.rate`), which requires `--yes`.
  - **Args**: `--order <UUID>`, `--rating <1-5>` (required; anything else is `invalid_arguments`, exit 2), `--yes`.
  - **Output**: `data.order_id`, `data.rating` (a JSON integer), `data.trade_pubkey`, `data.node_pubkey`, after `rate-received` with the same rating. A refusal is `node_refused` (`invalid_order_status`). A side that already rated, or a full-privacy counterparty, gets no answer from the node: `uncertain` (`reply_timeout`).
  - **Migration**: `rate -o|--orderid <UUID> -r|--rating <N>` (older documents wrote `--order-id`) is gone with no alias; use `order rate --order <UUID> --rating <N> [--yes]`.
  - **Handler**: `commands::order::rate::order_rate`; dispatch in `cli/held.rs`.

### Removed legacy commands (release notes)

Material for the release notes of the version that ships specs 009-015. `CHANGELOG.md` is generated by git-cliff from the commits at release
time, so this list lives here. None of the removed forms has an alias; each now exits 2 (`invalid_arguments` in JSON, a fixed line in human
mode) without running anything.

| Removed form | Replacement | Notes |
| --- | --- | --- |
| `dispute -o <UUID>`, `dispute --orderid <UUID>` (documented as `--order-id`) | `dispute open --order <UUID> [--yes]` | JSON `dispute.open`, requires `--yes`; the result carries the dispute id; the local status becomes `dispute`; a second dispute gets no answer from the node (`uncertain`) |
| `rate -o <UUID> -r <N>`, `rate --orderid <UUID> --rating <N>` (documented as `--order-id`) | `order rate --order <UUID> --rating <N> [--yes]` | JSON `order.rate`, requires `--yes`; `N` outside 1-5 is refused before anything is sent; a second rating gets no answer (`uncertain`) |

Added in the same range, for the notes: `identity init`, `identity show`, `order mine`, `order new --expires-in`, `--invoice -` on the four
invoice commands, `order chat` (`order.chat`), `order watch` (`order.watch`). The canonical JSON names `dispute.open` and `order.rate` are unchanged from the registry; only their paths changed.

---

If you add a new variant to the `Commands` enum:

1. Add the subcommand and its arguments in `src/cli/commands.rs`.
2. Add its handler function in an appropriate `src/cli/*.rs` module.
3. Extend the `Commands::run` match in `src/cli/legacy.rs` to call the handler.
4. Update this `commands.md` file so documentation stays in sync for humans and AI tools.

