# Mostro CLI 🧌

![Mostro-logo](static/logo.png)

A command-line client for [Mostro](https://github.com/MostroP2P/mostro), the P2P Bitcoin/Lightning exchange that runs over Nostr. With `mostro-cli` you can browse the orderbook, create and take orders, complete trades, open and resolve disputes, and act as an admin/solver — all from your terminal.

> **New to Mostro?** Mostro is a non-custodial P2P exchange protocol. Sellers lock sats in a Lightning hold invoice, buyers send fiat directly to sellers off-chain, and the Mostro daemon coordinates the trade over encrypted Nostr direct messages. The CLI is one of several clients (alongside mobile apps). See the [Mostro documentation](https://mostro.network) for protocol details.

---

## Table of Contents

- [Requirements](#requirements)
- [Installation](#installation)
- [How identities and keys work](#how-identities-and-keys-work)
- [Configuration](#configuration)
- [Quick start](#quick-start)
- [Trading: selling sats step by step](#trading-selling-sats-step-by-step)
- [Trading: buying sats step by step](#trading-buying-sats-step-by-step)
- [Direct messages with your counterpart](#direct-messages-with-your-counterpart)
- [Disputes (as a user)](#disputes-as-a-user)
- [Admin / Solver usage](#admin--solver-usage)
- [Backup, recovery and multi-device](#backup-recovery-and-multi-device)
- [Command reference](#command-reference)
- [Files, environment and where things live](#files-environment-and-where-things-live)
- [Troubleshooting / FAQ](#troubleshooting--faq)

---

## Requirements

- **Rust 1.86 or higher.** That is the minimum the CI verifies a build against. Note that `rust-toolchain.toml` pins `1.89.0` as the development toolchain, so building from a clone with `rustup` installed will fetch 1.89 regardless; 1.86 is the floor for `cargo install mostro-cli`.
- **A Mostro node to connect to** — its pubkey is mandatory configuration. See [Choosing a Mostro instance](#choosing-a-mostro-instance).
- A **Lightning wallet** to pay/receive hold invoices and regular invoices.
- Network access to public Nostr relays.

### Linux system dependencies (Ubuntu / Pop!_OS / Debian)

```bash
sudo apt update
sudo apt install -y cmake build-essential pkg-config libssl-dev
```

On macOS the Xcode command-line tools are usually enough (`xcode-select --install`). On Windows, use WSL2 or install the MSVC build tools.

---

## Installation

### Option A — from crates.io (recommended)

```bash
cargo install mostro-cli
```

This drops a `mostro-cli` binary into `~/.cargo/bin` (make sure that's on your `$PATH`).

### Option B — build from source

```bash
git clone https://github.com/MostroP2P/mostro-cli.git
cd mostro-cli
cargo build --release
# The binary will be at target/release/mostro-cli
```

Verify the install:

```bash
mostro-cli --version
mostro-cli --help
```

---

## How identities and keys work

This is the part most users skip, and then get confused about. Read it once and the rest of the CLI makes sense.

### You do not bring your own `nsec`

**There is nothing to set up.** `mostro-cli` generates and manages your Nostr keys for you on first run. You never paste an `nsec` for normal trading, and there is no key-generation step to perform beforehand (no `rana`, no `nostr-tool`, no wallet export).

In particular, these variables are **not** read by the CLI and setting them does nothing:

| Variable people try | Reality |
|---|---|
| `NSEC_PRIVKEY` | Obsolete. Removed when the CLI moved to mnemonic-derived keys (NIP-06). |
| `MOSTROPUBKEY` | Wrong name — the variable is `MOSTRO_PUBKEY`, with an underscore. |
| `PRIVKEY`, `NSEC` | Never existed. |

The only key you ever supply by hand is `ADMIN_NSEC`, and only if you are a solver/admin — see [Admin / Solver usage](#admin--solver-usage).

### The mnemonic (your master backup)

On first run, the CLI generates a **BIP39 12-word mnemonic** and stores it in a local SQLite database (`~/.mcli/mcli.db`, table `users`). This mnemonic is the seed for everything: lose it and you cannot recover orders or trade keys; share it and someone else can impersonate you.

### Three kinds of keys derived from that mnemonic

Mostro uses BIP32 hierarchical derivation (via NIP-06) to derive an unlimited number of Nostr keys from a single mnemonic:

| Key | Derivation index | What it does |
|---|---|---|
| **Identity key (`i0_pubkey`)** | `index = 0` | Your stable "account" pubkey. Mostro indexes users by this. Used for restore, ratings, last-trade-index queries. |
| **Trade keys** | `index = 1, 2, 3, ...` | A fresh keypair per order, for privacy. Each order in the local DB stores which index it used. |
| **Admin key (`ADMIN_NSEC`)** | Not derived from the mnemonic | A separate `nsec` provided via env var, only for admin/solver commands. See [Admin / Solver usage](#admin--solver-usage). |

The mnemonic-based user and the admin key are completely independent. You can run normal trades and admin commands from the same machine without conflict.

### What this means in practice

- **Your first command (e.g. `listorders`) creates your user** — no separate "init" step needed.
- **You can derive the same keys on another machine** by restoring the mnemonic (see [Backup, recovery and multi-device](#backup-recovery-and-multi-device)).
- **An `nsec` alone is not enough** to regenerate the trade keys: you need the full mnemonic, because BIP32 needs the chain code that a leaf `nsec` doesn't carry.

---

## Configuration

`mostro-cli` reads its configuration from environment variables (or equivalent CLI flags). The Mostro pubkey and at least one relay are mandatory. **Your own keys are not part of the configuration** — see [You do not bring your own `nsec`](#you-do-not-bring-your-own-nsec).

### Required

| Variable | CLI flag | Description |
|---|---|---|
| `MOSTRO_PUBKEY` | `-m, --mostropubkey` | The `npub` (or hex) of the Mostro instance you want to use. |
| `RELAYS` | `-r, --relays` | Comma-separated `wss://` Nostr relay URLs. |

### Optional

| Variable | CLI flag | Description |
|---|---|---|
| `POW` | `-p, --pow` | Proof-of-work difficulty (bits) required by the Mostro instance for incoming events. Set this if the daemon enforces PoW. |
| `SECRET` | `-s, --secret` | Use secret/anonymous mode for the inner event tuple (advanced, hides trade index from gift-wrap inner). |
| `TRANSPORT` | `-t, --transport` | Wire transport: `gift-wrap` (protocol v1) or `nip44` (protocol v2). Leave unset to auto-detect from the instance's info event. |
| `ADMIN_NSEC` | — | Admin/solver private key in `nsec1...` or hex format. Only read when an `adm*` command is invoked. |
| `MOSTRO_RPC_URL` | `http://127.0.0.1:50051` | `mostrod` admin gRPC endpoint (`[rpc]` in the daemon's settings). Only used by `admsetmaintenance` / `admmaintenancestatus` / `admcancelpending`. |
| `MOSTRO_RPC_TOKEN` | — | Bearer token for the admin gRPC, required when the daemon sets `[rpc].auth_token`. Only used by the three commands above. Sent in cleartext only to a loopback URL (direct or through an SSH tunnel); any other `http://` host is refused, use `https://` via a TLS proxy instead. |
| `RUST_LOG` | `-v, --verbose` | **Not actually configurable.** The logger is initialised only when `-v` is passed, and `-v` overwrites `RUST_LOG` with `info` first. So `RUST_LOG` alone produces no output, and `RUST_LOG=debug -v` still logs at `info`. `-v` is the only available level. |

### Choosing a Mostro instance

`mostro-cli` is only a client: it does not ship with a default node, and this README deliberately does not hard-code one. Mostro is a federation of independently operated daemons — any given instance can go offline, change its pubkey, or stop serving your currency at any time, so a pubkey pasted here would eventually send you to a dead node.

**Use the pubkey of the Mostro node you trust.** Ways to get one:

- **Ask the operator.** If you already trade on a given instance (through a mobile client, a community, or a friend), ask for its `npub` and the relays it publishes to.
- **Discover instances on Nostr.** Every running daemon publishes an addressable info event of **kind `38385`**, tagged with its `mostro_version`, `protocol_version`, `fee`, `pow` and `max_order_amount`. Querying a relay for that kind lists the instances it knows about, and the event's author pubkey is the value you need for `MOSTRO_PUBKEY`. Orders themselves are kind `38383` events authored by the same pubkey. Any Nostr client or CLI that can filter by kind will do.
- **Run your own.** The daemon is open source: [github.com/MostroP2P/mostro](https://github.com/MostroP2P/mostro). Running it yourself is also the recommended way to test the whole flow (including on testnet) without touching a stranger's node — its config file holds the pubkey and relays you then feed to `mostro-cli`.

Whichever you pick, `RELAYS` must include at least one relay that the instance actually publishes to, otherwise the CLI connects successfully and simply sees nothing.

### Suggested setup

Create a small env file you `source` before using the CLI:

```bash
# ~/.config/mostro/env.sh   (chmod 600)
export MOSTRO_PUBKEY="<npub-of-your-mostro-node>"
export RELAYS="wss://<relay-your-node-publishes-to>,wss://<another-relay>"
# export POW=10                # only if the node enforces proof of work
# export ADMIN_NSEC=nsec1...   # only if you're an admin/solver
```

```bash
source ~/.config/mostro/env.sh
mostro-cli listorders
```

> Replace both placeholders with the real values of the instance you want to trade on — see [Choosing a Mostro instance](#choosing-a-mostro-instance). No networked command works until `MOSTRO_PUBKEY` points at a live node (`--version` and `--help` are the only exceptions).

### About `.env` files

`mostro-cli` does **not** load a `.env` file automatically — there is no dotenv support in the binary, so dropping a `.env` next to the executable has no effect. Older guides (and some issue comments) suggest it; that advice is outdated.

If you prefer keeping settings in a `.env`-style file, export them yourself before running the CLI:

```bash
# ~/.config/mostro/.env   (chmod 600)
MOSTRO_PUBKEY=<npub-of-your-mostro-node>
RELAYS=wss://<relay-your-node-publishes-to>
POW=0
```

```bash
set -a; source ~/.config/mostro/.env; set +a
mostro-cli listorders
```

`set -a` marks every variable assigned by the file for export, so the CLI sees them; `set +a` turns that back off.

---

## Quick start

Export your configuration first — every command below fails immediately without it:

```bash
export MOSTRO_PUBKEY="<npub-of-your-mostro-node>"
export RELAYS="wss://<relay-your-node-publishes-to>"
```

```bash
# 1. List open orders
mostro-cli listorders

# 2. Filter by kind, currency or status
mostro-cli listorders -k sell -c usd
mostro-cli listorders -k buy -c ves -s pending

# 3. Inspect details for specific orders
mostro-cli ordersinfo -o <uuid-1> -o <uuid-2>

# 4. Create your own order (sell 1000-10000 ARS at market price)
mostro-cli neworder -k sell -c ars -f 1000-10000 -m "face to face"

# 5. Take someone else's sell order
mostro-cli takesell -o <order-id> -a 500     # optional fiat amount for range orders

# 6. After the trade, fetch new DMs from Mostro
mostro-cli getdm --since 60
```

On the very first run you will see something like:

```
Creating database file with orders table...
User created with pubkey: <your i0_pubkey>
```

Write down or back up the mnemonic before doing anything else — see [Backup, recovery and multi-device](#backup-recovery-and-multi-device).

### The CLI does not stay connected — you poll

This is the biggest difference from a mobile Mostro client. Each `mostro-cli` invocation connects to the relays, sends (or reads) what you asked for, prints the result and exits. It does **not** keep running to notify you when your counterpart acts.

So a trade is driven by you re-running `getdm`:

```bash
mostro-cli getdm --since 60     # everything Mostro sent you in the last 60 minutes
```

Run it after every step where you are waiting on the other side — an order being taken, an invoice arriving, fiat being marked as sent, sats being released. `--since` defaults to 30 minutes; widen it if you have been away.

Pending orders also expire (24 hours on a typical instance — the exact value is the `expiration_hours` tag of the node's info event). If nobody takes your order before then, it disappears from the orderbook and any locked sats are returned; you can pass `--expiration-days N` to `neworder` to request a different window.

---

## Trading: selling sats step by step

This is the seller flow when you create the order (maker, sell).

1. **Create the order**

   ```bash
   mostro-cli neworder -k sell -c usd -f 100 -m "wise,strike" -a 0
   ```

   - `-k sell` — you are selling sats.
   - `-c usd` — fiat currency code.
   - `-f 100` — fiat amount (use `-f 100-500` for a range order).
   - `-m "wise,strike"` — comma-separated payment methods.
   - `-a 0` — sats amount (`0` = market price at trade time).
   - `-p 2` — optional price premium percentage.
   - `--expiration-days N` — optional custom expiration.

2. **Mostro replies with a hold invoice.** Pay it with your Lightning wallet. Funds are locked, not transferred yet.

3. **Wait for a buyer to take the order.** Check messages:

   ```bash
   mostro-cli getdm --since 60          # last 60 minutes
   ```

4. **Buyer adds an invoice (if they didn't include one when taking)** — Mostro forwards their invoice.

5. **Buyer marks fiat as sent.** You'll see a `fiat-sent` message via `getdm`. **Confirm you actually received the fiat** outside the CLI before releasing.

6. **Release the hold invoice** so the buyer gets the sats:

   ```bash
   mostro-cli release -o <order-id>
   ```

7. **Rate your counterpart:**

   ```bash
   mostro-cli rate -o <order-id> -r 5
   ```

If something goes wrong before release, you can `cancel` (only valid in pending state) or `dispute`. See [Disputes](#disputes-as-a-user).

---

## Trading: buying sats step by step

Buyer-as-taker flow against an existing sell order:

1. **Find a sell order:**

   ```bash
   mostro-cli listorders -k sell -c usd
   ```

2. **Take it.** You can either provide a Lightning invoice for the trade amount, or omit it and add one later:

   ```bash
   mostro-cli takesell -o <order-id> -i lnbc...    # with invoice
   mostro-cli takesell -o <order-id>               # without invoice
   ```

   For range orders, also pass `-a <fiat_amount>`.

3. **If you didn't provide an invoice, add one when prompted:**

   ```bash
   mostro-cli addinvoice -o <order-id> -i lnbc...
   ```

4. **Pay the seller in fiat** using the agreed payment method.

5. **Tell Mostro fiat is sent:**

   ```bash
   mostro-cli fiatsent -o <order-id>
   ```

6. **Wait for the seller to release.** Check `getdm`. When they release, Mostro pays your invoice.

7. **Rate the seller:**

   ```bash
   mostro-cli rate -o <order-id> -r 5
   ```

### Buying as a maker

If you want to *post* a buy order instead of taking one, use `neworder -k buy`. You'll typically include a Lightning Address as the invoice (`-i your@walletofsatoshi.com`) so the seller knows where to pay you.

---

## Direct messages with your counterpart

Every order has a counterparty pubkey. You can chat over NIP-17 gift-wrapped DMs:

```bash
# Get the conversation key for a counterpart (informational)
mostro-cli conversationkey -p <their-pubkey>

# Read DMs from Mostro (default) or directly from the counterpart
mostro-cli getdm --since 30
mostro-cli getdm --since 30 --from-user

# Get DMs received by the trade key of a specific order
mostro-cli getdmuser -p <their-pubkey> -o <order-id> --since 120

# Send a DM (uses the order's trade key)
mostro-cli senddm -p <their-pubkey> -o <order-id> -m "hi, sending now"

# Send a gift-wrapped DM to a user (similar, alternative encoding)
mostro-cli dmtouser -p <their-pubkey> -o <order-id> -m "hello"
```

---

## Disputes (as a user)

If your counterpart misbehaves (no fiat received, no release after fiat sent, etc.):

```bash
mostro-cli dispute -o <order-id>
```

This puts the order in dispute. A solver will be assigned and contact you. Use `getdm` to receive their messages and respond with `senddm`. Be honest, provide evidence, and respect that the solver decides.

To see the public dispute queue:

```bash
mostro-cli listdisputes
```

---

## Admin / Solver usage

Admin commands let an authorized solver settle or cancel disputed orders, take disputes from the queue, send admin DMs to users, and add new solvers. They are **only useful if your pubkey is already registered with the Mostro daemon** — either as the root admin (in `mostrod`'s settings) or as a solver added via `admaddsolver`.

### Important: admin keys are completely independent

- Your trade activity uses the mnemonic-derived user in `~/.mcli/mcli.db`.
- Admin commands use the `nsec` from the **`ADMIN_NSEC` environment variable**.
- Nothing is stored on disk for the admin key. Set it only when you need it.

You can be a regular user and a solver on the same machine; just keep both wallets/keys separate.

### Setup

1. Make sure the daemon operator has registered your pubkey, either by including it as the admin pubkey in `mostrod`'s config or by running `admaddsolver` from an existing admin's CLI with your `npub`.

2. Put your solver `nsec` in an env var (use a leading space to keep it out of shell history):

   ```bash
    export ADMIN_NSEC="nsec1xxxxxxxxxxxxxxxxxxxxxxxxxxxx"
   ```

   Or pass it inline per command:

   ```bash
    ADMIN_NSEC="nsec1..." mostro-cli admsettle -o <order-id>
   ```

3. Run any admin subcommand. The CLI only reads `ADMIN_NSEC` when one of these is invoked:

   - `admsettle`, `admcancel`
   - `admaddsolver`, `admtakedispute`
   - `admsenddm`, `getadmindm`

   For every other command, `ADMIN_NSEC` is ignored. Note that `sendadmindmattach` is **not** on this list despite its name: it signs with the trade key of the order you pass in, so it works without `ADMIN_NSEC` set.

### Admin commands

```bash
# Take a pending dispute (from listdisputes)
mostro-cli admtakedispute -d <dispute-id>

# Settle the seller's hold invoice (pays the buyer)
mostro-cli admsettle -o <order-id>

# Cancel a disputed order (returns the seller's locked sats)
mostro-cli admcancel -o <order-id>

# Bond slashing (anti-abuse, phase 2): add --slash-seller and/or --slash-buyer
mostro-cli admsettle -o <order-id> --slash-buyer
mostro-cli admcancel -o <order-id> --slash-seller

# Add a new solver
mostro-cli admaddsolver -n <npub-of-new-solver>

# Read DMs sent to your admin pubkey
mostro-cli getadmindm --since 120

# DM a user with your admin identity
mostro-cli admsenddm -p <user-pubkey> -m "hi, I'm the solver assigned to your dispute"

# Send an admin DM with an encrypted attachment (uploaded to Blossom)
mostro-cli sendadmindmattach -p <user-pubkey> -o <order-id> -f /path/to/evidence.pdf
```

### Operator commands: maintenance mode (Lightning node migration)

These commands talk to the daemon's admin gRPC directly instead of Nostr, so they need `MOSTRO_RPC_URL` (and `MOSTRO_RPC_TOKEN` if the daemon requires it) but **not** `ADMIN_NSEC`, relays or a mnemonic. `admsetmaintenance` must run on the daemon's host or through a tunnel to it: `mostrod` accepts `SetMaintenanceMode` from loopback peers only. `admmaintenancestatus` (read-only) and `admcancelpending` (bearer token when configured) have no peer restriction, so they also work against a remote `https://` endpoint behind a TLS proxy.

```bash
# Close the book: new orders and takes are rejected, open trades keep working
mostro-cli admsetmaintenance --enabled true --reason "LN node migration"

# Watch the drain; switch the Lightning node only once drained = true
mostro-cli admmaintenancestatus

# Shorten the drain: cancel a still-pending order yourself (maker notified,
# its bond released at once). Announce it first — it is the user's order.
mostro-cli admcancelpending -o <order-id>

# Reopen the book
mostro-cli admsetmaintenance --enabled false
```

The full procedure (drain, stop, edit `[lightning]`, start, reopen) is in the daemon's `docs/LIGHTNING_OPS.md`, section "Migrating to a Different Lightning Node".

### Tips for solvers

- Always read both sides' DMs (`getadmindm` plus the order's chat history) before deciding.
- `admsettle` releases sats to the buyer; `admcancel` returns them to the seller. Pick based on who fulfilled their side.
- Bond slashing flags exist for the anti-abuse-bond phase 2 protocol — use them only when the daemon and your operator's policy support it.

---

## Backup, recovery and multi-device

### What to back up

The only file that matters is the **mnemonic**. Everything else (orders, indexes) can be re-derived from it.

To read the mnemonic from your local DB:

```bash
sqlite3 ~/.mcli/mcli.db "SELECT mnemonic FROM users;"
```

Store the 12 words offline (paper, metal, encrypted vault). Do **not** commit them to git or put them in plain text on shared machines.

### Moving to a new machine: copy the database

**If you have trades in flight, copy `~/.mcli/mcli.db` to the new machine.** This is the only path that lets you *continue* those trades, because commands like `release`, `cancel`, `addinvoice` and `senddm` look the order up in the local `orders` table and fail without it. The file contains no funds — only your mnemonic and cached order metadata — but it does contain the mnemonic, so move it over a secure channel and keep the `0600` permissions.

### Restoring from the mnemonic alone

Use this when the database is gone. It recovers your **identity**, not your local order history — read the limitation at the end of this section before relying on it mid-trade.

> **Let the CLI create the database — do not hand-craft it.** `mostro-cli` only creates its tables when `~/.mcli/mcli.db` does not yet exist. If you pre-create that file yourself with just a `users` table, the `orders` table is never created, `listorders` still appears to work, and the first command that touches an order fails with `no such table: orders`. Always run the CLI once first, then overwrite the mnemonic.

1. Install `mostro-cli` on the new machine.

2. **Configure the CLI first.** `MOSTRO_PUBKEY` and `RELAYS` are validated *before* the database is created, so without them the next step aborts and no database appears:

   ```bash
   export MOSTRO_PUBKEY="<npub-of-your-mostro-node>"
   export RELAYS="wss://<relay-your-node-publishes-to>"
   ```

3. **Run any command once** so the CLI builds a complete, correctly-permissioned database. It will generate a throwaway mnemonic that you are about to replace:

   ```bash
   mostro-cli listorders
   ```

4. **Overwrite the mnemonic** with your backed-up 12 words and clear the trade index that belonged to the throwaway user.

   Do not type the mnemonic as a command argument: it would land in your shell history and be visible to any local user running `ps`. Read it into a variable instead, with echo disabled, and let `sqlite3` take the statement on stdin:

   ```bash
   read -rs -p "mnemonic: " MNEMONIC && echo
   sqlite3 ~/.mcli/mcli.db <<SQL
   UPDATE users SET mnemonic = '$MNEMONIC', last_trade_index = NULL;
   SQL
   unset MNEMONIC
   ```

   The heredoc expands `$MNEMONIC` into `sqlite3`'s standard input, so the words never appear in `argv` or in history. BIP39 words are lowercase ASCII, so quoting is safe — but do check you pasted a mnemonic and not something containing a `'`.

5. **Sync the trade index. This step is required, not optional:**

   ```bash
   mostro-cli getlasttradeindex
   ```

   Trade keys are derived from an incrementing index, and the daemon rejects an index it has already seen. A freshly restored database starts back at index 1, so without this sync your next order is refused. The command asks Mostro for your real last index and writes it back to the local database.

6. **Ask Mostro what you have open:**

   ```bash
   mostro-cli restore
   ```

   This prints the identity pubkey it derived from your restored mnemonic, plus the ID, trade index and status of every active order and dispute Mostro holds for you.

7. **(Cosmetic) realign `i0_pubkey`.** The `users` row still carries the throwaway identity in its primary key column. Nothing derives from it — every identity and trade key comes from the `mnemonic` column at runtime, and `User::save` matches on whatever value is stored, so the database stays self-consistent. If you want the column to reflect reality anyway, take the `User` pubkey that `restore` printed in the previous step:

   ```bash
   sqlite3 ~/.mcli/mcli.db "UPDATE users SET i0_pubkey = '<pubkey printed by restore>';"
   ```

> **Limitation: `restore` does not rebuild your local order cache.** It reports what Mostro knows, but it does not insert those orders into the local `orders` table. Commands that operate on a specific order — `release`, `cancel`, `fiatsent`, `addinvoice`, `rate`, `senddm` — read that table first and will fail on an order that is not in it. So a mnemonic-only restore gets your identity and your ratings back and lets you trade again from scratch, but it cannot resume a trade that was already in flight. For that, copy the database (see above).

> A friendlier `import-mnemonic` subcommand may land in the future. Until then, the flow above is the supported path.

---

## Command reference

Every command supports `-h, --help`. The list below is a one-line summary; run `mostro-cli <cmd> --help` for full flags.

### Order browsing & creation
- `listorders [-s status] [-c currency] [-k kind]` — list open orders.
- `ordersinfo -o <uuid> [-o <uuid> ...]` — request details for specific orders.
- `neworder -k <buy|sell> -c <fiat> -f <amount|min-max> -m <methods> [-a <sats>] [-p <premium>] [-i <invoice>] [--expiration-days N]` — create an order.

### Taking orders
- `takesell -o <id> [-i <invoice>] [-a <fiat-amount>]` — buyer takes a sell order.
- `takebuy -o <id> [-a <fiat-amount>]` — seller takes a buy order.
- `addinvoice -o <id> -i <invoice>` — buyer adds an invoice after taking.

### Trade lifecycle
- `fiatsent -o <id>` — buyer confirms fiat sent.
- `release -o <id>` — seller releases the hold invoice.
- `cancel -o <id>` — cancel a pending order or cooperatively cancel later.
- `rate -o <id> -r <1-5>` — rate counterpart.
- `dispute -o <id>` — open a dispute.
- `addbondinvoice -o <id> -i <invoice>` — reply to a bond payout request with an invoice for your share of a slashed bond.

### Messaging
- `getdm [--since <min>] [--from-user]` — fetch recent DMs.
- `getdmuser -p <pubkey> -o <id> [--since <min>]` — DMs to a specific order's trade key.
- `senddm -p <pubkey> -o <id> -m <message>` — DM your counterpart.
- `dmtouser -p <pubkey> -o <id> -m <message>` — gift-wrapped DM.
- `conversationkey -p <pubkey>` — show the conversation key.

### Disputes (read-only for users)
- `listdisputes` — public dispute queue.

### Admin / Solver (require `ADMIN_NSEC`)
- `admsettle -o <id> [--slash-seller] [--slash-buyer]`
- `admcancel -o <id> [--slash-seller] [--slash-buyer]`
- `admtakedispute -d <dispute-id>`
- `admaddsolver -n <npub>`
- `admsenddm -p <pubkey> -m <msg>`
- `getadmindm [--since <min>] [--from-user]`

### Operator (admin gRPC: `MOSTRO_RPC_URL` / `MOSTRO_RPC_TOKEN`, no `ADMIN_NSEC`)
- `admsetmaintenance --enabled <true|false> [--reason <text>]`
- `admmaintenancestatus`
- `admcancelpending -o <id>` — cancel a still-pending order, releasing its bonds; the daemon refuses any other status.

### Solver tooling (no `ADMIN_NSEC` needed)
- `sendadmindmattach -p <pubkey> -o <id> -f <file>` — send an encrypted file attachment (uploaded to a Blossom server) over the order's trade key.

### Identity / recovery
- `restore` — re-sync active orders and disputes from Mostro.
- `getlasttradeindex` — fetch your last known trade index from Mostro.
- `getlasttradeprivkey` — show the private key for the last trade index (advanced).

### Global flags

> **These must come *before* the subcommand.** They are parsed on the top-level command, so `mostro-cli listorders -m <npub>` fails with `error: unexpected argument '-m' found`. Write `mostro-cli -m <npub> listorders` instead. This also avoids clashing with subcommand flags that reuse the same letters (`-m` is `--payment-method` on `neworder` and `--message` on `senddm`, `-p` is `--premium` on `neworder` and `--pubkey` on the DM commands).

- `-v, --verbose` — enable info logging. This is the only log control; it overwrites `RUST_LOG` with `info`.
- `-m, --mostropubkey <npub>` — overrides `MOSTRO_PUBKEY`.
- `-r, --relays <list>` — overrides `RELAYS`.
- `-p, --pow <bits>` — overrides `POW`.
- `-s, --secret` — secret mode for inner event tuple.
- `-t, --transport <gift-wrap|nip44>` — overrides `TRANSPORT` (auto-detected when unset).

```bash
mostro-cli -m <npub> -r wss://<relay> listorders -k sell -c usd
```

---

## Files, environment and where things live

| Path | What it is |
|---|---|
| `~/.mcli/` | The CLI's data directory. Created on first run. |
| `~/.mcli/mcli.db` | SQLite database with your `users` row (mnemonic, identity key, last trade index) and `orders` cache. |

Environment variables read by the CLI:

| Var | Purpose |
|---|---|
| `MOSTRO_PUBKEY` | Required — Mostro instance pubkey. |
| `RELAYS` | Required — Nostr relays. |
| `POW` | Optional — proof-of-work bits. |
| `SECRET` | Optional — `true` enables secret-mode inner tuple. |
| `TRANSPORT` | Optional — `gift-wrap` or `nip44`; auto-detected when unset. |
| `ADMIN_NSEC` | Optional — only used by admin commands. |
| `RUST_LOG` | Read but effectively not configurable — `-v` overwrites it with `info` and is the only thing that initialises the logger. |

The database stores **secret material** (your mnemonic). Treat `~/.mcli/mcli.db` like a wallet seed file:

- Set restrictive permissions: `chmod 600 ~/.mcli/mcli.db`.
- Don't sync it via clear-text cloud backups.
- Don't share the file or the mnemonic with anyone.

---

## Troubleshooting / FAQ

**`Invalid secret key` / `Failed to parse ADMIN_NSEC`** — Only admin commands parse a key you supply, and the only one they read is `ADMIN_NSEC`; check it is a well-formed `nsec1...` or hex private key. Normal commands never parse a user-supplied key at all — they derive theirs from the mnemonic in `~/.mcli/mcli.db`, so for those you only need `MOSTRO_PUBKEY` and `RELAYS`. Older releases read an `NSEC_PRIVKEY` variable and failed this way when it was malformed; current versions ignore it entirely. See [You do not bring your own `nsec`](#you-do-not-bring-your-own-nsec).

**"How do I generate my keys?"** — You don't. There is no key-generation step and no need for tools like `rana`. The first command you run creates `~/.mcli/mcli.db` with a fresh BIP39 mnemonic; every identity and trade key is derived from it (NIP-06). Back the mnemonic up — see [Backup, recovery and multi-device](#backup-recovery-and-multi-device).

**`MOSTRO_PUBKEY not set`** — Export it, or pass `-m <npub>` **before** the subcommand (`mostro-cli -m <npub> listorders`, not `mostro-cli listorders -m <npub>` — see [Global flags](#global-flags)). Mind the underscore: only `MOSTRO_PUBKEY` is read, `MOSTROPUBKEY` is not. If you don't have a node pubkey yet, see [Choosing a Mostro instance](#choosing-a-mostro-instance).

**`RELAYS not set`** — `RELAYS` is required too, and that exact name is the one the CLI reads. Export it (comma-separated `wss://` URLs) or pass `-r <relay[,relay...]>` before the subcommand.

**`error: unexpected argument '-m' found`** (or `-r`, `-p`, `-t`, `-v`) — Global flags belong before the subcommand: `mostro-cli -m <npub> listorders`. Placed after it, clap parses them against the subcommand, which either rejects them or silently means something else. See [Global flags](#global-flags).

**My `.env` file is ignored** — It is not loaded automatically; the CLI has no dotenv support. Use `set -a; source .env; set +a` first — see [About `.env` files](#about-env-files).

**`ADMIN_NSEC not set (required for admin commands)`** — Only admin subcommands need it. Export it in the same shell, or prefix the command: `ADMIN_NSEC=nsec1... mostro-cli admsettle ...`.

**`listorders` returns nothing** — Almost always a configuration problem rather than an empty orderbook. In order:

1. **Is `MOSTRO_PUBKEY` a live instance?** A node that has been shut down, or a pubkey copied from an outdated guide, produces exactly this: a clean connection and zero orders. Confirm the pubkey with its operator, or look for its kind-`38385` info event on the relay — see [Choosing a Mostro instance](#choosing-a-mostro-instance).
2. **Do your relays carry that instance?** The node only publishes to the relays it is configured with. A perfectly healthy relay that the node never writes to will show nothing.
3. **Is the relay reachable?** Test with e.g. `websocat wss://<your-relay>`.
4. **Are your filters too narrow?** `-k`, `-c` and `-s` combine; drop them and retry.

Run with `-v` (before the subcommand) for relay-level logs.

**Mostro rejects events / no reply** — The instance may require `POW`. Ask the operator what difficulty is enforced and export `POW=<bits>`.

**`no such table: orders`** — Your `~/.mcli/mcli.db` was created by something other than the CLI (usually by hand-crafting it while restoring a mnemonic). The CLI only creates its tables when that file does not exist, so a pre-made database is missing `orders`. Delete it and follow [Restoring from the mnemonic alone](#restoring-from-the-mnemonic-alone) — but back up the mnemonic first: `sqlite3 ~/.mcli/mcli.db "SELECT mnemonic FROM users;"`.

**Mostro rejects my order after restoring on a new machine** — You very likely skipped the trade-index sync. Run `mostro-cli getlasttradeindex`; see [Restoring from the mnemonic alone](#restoring-from-the-mnemonic-alone).

**Nothing happens / I'm waiting for my counterpart** — The CLI does not stay connected. Re-run `mostro-cli getdm --since <minutes>` to pull new messages; see [The CLI does not stay connected — you poll](#the-cli-does-not-stay-connected--you-poll).

**Lost the database / changed machine** — See [Backup, recovery and multi-device](#backup-recovery-and-multi-device). Without the mnemonic you cannot recover anything; with the mnemonic you recover your identity but not in-flight trades (see below).

**I restored my mnemonic but `release` / `addinvoice` / `senddm` says the order doesn't exist** — Expected. `mostro-cli restore` reports the orders Mostro holds for you, but it does not write them into the local `orders` table, and those commands look the order up there first. A mnemonic-only restore cannot resume a trade that was already in flight — copying `~/.mcli/mcli.db` is the only way to do that. See [Moving to a new machine: copy the database](#moving-to-a-new-machine-copy-the-database).

**Multiple orders in flight** — Each gets its own derived trade key. The DB tracks them; just keep using order IDs.

**"Where is my mnemonic?"** — `sqlite3 ~/.mcli/mcli.db "SELECT mnemonic FROM users;"`. Back it up offline.

**Migrating from older versions** — Legacy `buyer_token` / `seller_token` columns are dropped automatically on startup; no action needed.

---

## Progress overview

- [x] Displays order list
- [x] Take orders (buy & sell)
- [x] Post orders (buy & sell, including range orders)
- [x] Full sell and buy flows
- [x] Maker cancel pending order
- [x] Cooperative cancellation
- [x] Buyer: add a new invoice if payment fails
- [x] Rate users
- [x] Dispute flow (users)
- [x] Dispute management (admins / solvers)
- [x] Create buy orders with Lightning Address
- [x] Direct messages with peers (NIP-17)
- [x] Conversation key management
- [x] Add new dispute solvers (admins)
- [x] Identity management (NIP-06)
- [x] List own orders
- [x] Bond slashing flags on admin settle/cancel (anti-abuse phase 2)
- [x] Encrypted admin DM attachments (Blossom)

---

## Contributing

Issues and PRs welcome at [github.com/MostroP2P/mostro-cli](https://github.com/MostroP2P/mostro-cli). Please open an issue first for non-trivial changes so we can discuss the approach.

## License

See [LICENSE](LICENSE).
