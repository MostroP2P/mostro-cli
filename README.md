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

- **Rust** 1.74 or higher (recommended — anything newer than 1.64 should compile).
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

`mostro-cli` reads its configuration from environment variables (or equivalent CLI flags). The Mostro pubkey and at least one relay are mandatory.

### Required

| Variable | CLI flag | Description |
|---|---|---|
| `MOSTRO_PUBKEY` | `-m, --mostropubkey` | The `npub` (or hex) of the Mostro instance you want to use. |
| `RELAYS` | `-r, --relays` | Comma-separated `wss://` Nostr relay URLs. |

### Optional

| Variable | CLI flag | Description |
|---|---|---|
| `POW` | `-p, --pow` | Proof-of-work difficulty (bits) required by the Mostro instance for incoming events. Set this if the daemon enforces PoW. |
| `SECRET` | `--secret` | Use secret/anonymous mode for the inner event tuple (advanced, hides trade index from gift-wrap inner). |
| `ADMIN_NSEC` | — | Admin/solver private key in `nsec1...` or hex format. Only read when an `adm*` command is invoked. |
| `RUST_LOG` | `-v, --verbose` | Verbose logging. The `-v` flag sets `RUST_LOG=info` for you. |

### Suggested setup

Create a small env file you `source` before using the CLI:

```bash
# ~/.config/mostro/env.sh   (chmod 600)
export MOSTRO_PUBKEY="npub1ykvsmrmw2hk7jgxgy64zr8tfkx4nnjhq9eyfxdlg3caha3ph0skq6jr3z0"
export RELAYS="wss://relay.mostro.network,wss://relay.damus.io"
# export POW=10
# export ADMIN_NSEC=nsec1...   # only if you're an admin/solver
```

```bash
source ~/.config/mostro/env.sh
mostro-cli listorders
```

> Pubkeys above are illustrative — replace them with the actual Mostro instance and relays you want to trade on.

---

## Quick start

```bash
# 1. List open orders
mostro-cli listorders

# 2. Filter by kind, currency or status
mostro-cli listorders -k sell -c usd
mostro-cli listorders -k buy -c ves -s pending

# 3. Inspect details for specific orders
mostro-cli ordersinfo -o <uuid-1> -o <uuid-2>

# 4. Create your own order (sell 1000 ARS, range allowed)
mostro-cli neworder -k sell -c ars -f 1000-10000 -m "face to face"

# 5. Take someone else's sell order
mostro-cli takesell -o <order-id> -a 500     # optional fiat amount for range orders

# 6. After the trade, fetch new DMs from Mostro
mostro-cli getdm --since 60
```

On the very first run you will see something like:

```
Directory /home/user/.mcli created.
Creating database file with orders table...
User created with pubkey: <your i0_pubkey>
```

Write down or back up the database / mnemonic before doing anything else.

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

   For non-admin commands, `ADMIN_NSEC` is ignored.

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

### Restoring on a new machine

1. Install `mostro-cli` on the new machine.
2. **Before running any command**, create `~/.mcli/mcli.db` with your mnemonic pre-inserted, or stop after the first auto-init and manually overwrite the row in `users`. A simple way using sqlite3:

   ```bash
   mkdir -p ~/.mcli
   sqlite3 ~/.mcli/mcli.db <<'SQL'
   CREATE TABLE IF NOT EXISTS users (
     i0_pubkey char(64) PRIMARY KEY,
     mnemonic TEXT,
     last_trade_index INTEGER,
     created_at INTEGER
   );
   SQL
   # Then insert your mnemonic (replace the values):
   sqlite3 ~/.mcli/mcli.db "INSERT INTO users (i0_pubkey, mnemonic, created_at) VALUES ('<your-i0_pubkey-hex>', '<your 12 words>', strftime('%s','now'));"
   ```

3. Run `mostro-cli restore`. This asks Mostro to resend the state of all your active orders and disputes so the new machine can rejoin the conversations.

4. (Optional) sync the trade index:

   ```bash
   mostro-cli getlasttradeindex
   ```

> A friendlier `import-mnemonic` subcommand may land in the future. Until then, the manual flow above is the supported path.

### Backing up the whole DB

If you also want to preserve cached order metadata and avoid re-fetching, copy `~/.mcli/mcli.db` to the new machine instead. The DB contains no funds — only Nostr keys and order metadata.

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
- `sendadmindmattach -p <pubkey> -o <id> -f <file>`
- `getadmindm [--since <min>] [--from-user]`

### Identity / recovery
- `restore` — re-sync active orders and disputes from Mostro.
- `getlasttradeindex` — fetch your last known trade index from Mostro.
- `getlasttradeprivkey` — show the private key for the last trade index (advanced).

### Global flags
- `-v, --verbose` — enable info logging.
- `-m, --mostropubkey <npub>` — overrides `MOSTRO_PUBKEY`.
- `-r, --relays <list>` — overrides `RELAYS`.
- `-p, --pow <bits>` — overrides `POW`.
- `--secret` — secret mode for inner event tuple.

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
| `ADMIN_NSEC` | Optional — only used by admin commands. |
| `RUST_LOG` | Optional — verbose logging level. |

The database stores **secret material** (your mnemonic). Treat `~/.mcli/mcli.db` like a wallet seed file:

- Set restrictive permissions: `chmod 600 ~/.mcli/mcli.db`.
- Don't sync it via clear-text cloud backups.
- Don't share the file or the mnemonic with anyone.

---

## Troubleshooting / FAQ

**`MOSTRO_PUBKEY not set`** — Export it or pass `-m <npub>`. Same for `RELAYS`.

**`ADMIN_NSEC not set (required for admin commands)`** — Only admin subcommands need it. Export it in the same shell, or prefix the command: `ADMIN_NSEC=nsec1... mostro-cli admsettle ...`.

**`listorders` returns nothing** — Check `RELAYS` connectivity (`websocat wss://relay.mostro.network`), confirm `MOSTRO_PUBKEY` matches the instance you actually want to trade on, and try `--verbose` for relay logs.

**Mostro rejects events / no reply** — The instance may require `POW`. Ask the operator what difficulty is enforced and export `POW=<bits>`.

**Lost the database / changed machine** — See [Backup, recovery and multi-device](#backup-recovery-and-multi-device). Without the mnemonic, active orders/disputes cannot be recovered.

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
