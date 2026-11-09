# Mostro CLI 🧌

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-yellow.svg)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-1.94%2B-blue.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/crates/v/mostro-cli)](https://crates.io/crates/mostro-cli)

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/MostroP2P/mostro-cli)

![Mostro-logo](static/logo.png)

A command-line client for [Mostro](https://github.com/MostroP2P/mostro), the P2P Bitcoin/Lightning exchange that runs over Nostr. With `mostro-cli` you can browse the orderbook, create and take orders, complete trades, open and resolve disputes, and act as an admin/solver — all from your terminal.

> **New to Mostro?** Mostro is a non-custodial P2P exchange protocol. Sellers lock sats in a Lightning hold invoice, buyers send fiat directly to sellers off-chain, and the Mostro daemon coordinates the trade over encrypted Nostr direct messages. The CLI is one of several clients (alongside mobile apps). See the [Mostro documentation](https://mostro.network) for protocol details.

> **Driving the CLI from a program or an AI agent?** The [mostro-cli book](https://mostro.network/mostro-cli-book/) ([source](https://github.com/MostroP2P/mostro-cli-book)) documents the JSON interface: the outcome envelope, every JSON command with examples, the error and warning codes, the trade flows, sample wrappers and a [guide for AI agents](https://mostro.network/mostro-cli-book/wrapper/ai_agents.html). The installed binary describes itself with `mostro-cli --output json schema` (0.17.0 or later; 0.16.3 and earlier have no JSON interface), which needs no node, identity or network.

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
- [Machine-readable output](#machine-readable-output)
- [Files, environment and where things live](#files-environment-and-where-things-live)
- [Troubleshooting / FAQ](#troubleshooting--faq)

---

## Requirements

- **Rust:** the declared minimum is 1.94 (required by the locked `sqlx` 0.9 dependency graph); `rust-toolchain.toml` pins 1.97.0 for development. The current refactor checkpoint was tested on native Linux with 1.97.0 and checked against 1.94.0; native Windows verification remains a separate gate. For a development checkout without trade credentials, follow the [Linux/WSL laptop setup](docs/ai-development.md#resume-on-another-linuxwsl-laptop).
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

### Option B — a prebuilt binary

Each [release](https://github.com/MostroP2P/mostro-cli/releases) carries binaries for Linux (x86_64 and aarch64 musl, ARM), FreeBSD,
Windows and macOS (`aarch64-apple-darwin` for Apple silicon, `x86_64-apple-darwin` for Intel), with their SHA-256 sums in `manifest.txt`.
Each file is named `mostro-cli-<target>` (with `.exe` on Windows). Download the one for your system, check its sum,
and install it as `mostro-cli` in a directory on your `PATH`. The install step runs only when the sum matches:

```bash
target=x86_64-unknown-linux-musl   # your target, e.g. aarch64-apple-darwin or x86_64-apple-darwin
base=https://github.com/MostroP2P/mostro-cli/releases/latest/download
curl -fL -O "$base/mostro-cli-$target" -O "$base/manifest.txt"
line="$(grep "/mostro-cli-$target\$" manifest.txt | cut -d' ' -f1)  mostro-cli-$target"
if [ "$(uname -s)" = Darwin ]; then echo "$line" | shasum -a 256 -c; else echo "$line" | sha256sum -c; fi &&
  sudo install -m 0755 "mostro-cli-$target" /usr/local/bin/mostro-cli   # or any directory on your PATH
```

On Windows, in PowerShell:

```powershell
$base = "https://github.com/MostroP2P/mostro-cli/releases/latest/download"
Invoke-WebRequest "$base/mostro-cli-x86_64-pc-windows-gnu.exe" -OutFile mostro-cli.exe -UseBasicParsing
Invoke-WebRequest "$base/manifest.txt" -OutFile manifest.txt -UseBasicParsing
$want = ((Select-String "windows-gnu\.exe$" manifest.txt).Line -split " ")[0]
if ((Get-FileHash mostro-cli.exe -Algorithm SHA256).Hash -eq $want) { "checksum OK" } else { throw "checksum mismatch" }
```

If it prints `checksum OK`, move `mostro-cli.exe` to a folder on your `PATH`.

The macOS binaries are not signed. A file downloaded with a browser instead of `curl` is quarantined, and Gatekeeper
blocks it: run `xattr -d com.apple.quarantine mostro-cli-<target>` before the install step.

### Option C — build from source

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

- **Create your user with `mostro-cli identity init`** (or let the first legacy identity-using command, e.g. `getadmindm`, create it): `order info`, `order new`, `message list` and the other migrated commands need an existing one and never create it. `identity init` needs no node or relay, never shows the mnemonic and refuses to touch an existing store; `mostro-cli identity show` prints your identity public key and last trade index. `order list` and `schema` are public and offline-safe: they never create or read the database or an identity.
- **You can derive the same keys on another machine** by restoring the mnemonic (see [Backup, recovery and multi-device](#backup-recovery-and-multi-device)).
- **An `nsec` alone is not enough** to regenerate the trade keys: you need the full mnemonic, because BIP32 needs the chain code that a leaf `nsec` doesn't carry.

---

## Configuration

`mostro-cli` reads its configuration from environment variables (or equivalent CLI flags); the commands that talk to a node also read the node settings from a [config file](#config-file) in the data directory. The Mostro pubkey and at least one relay are mandatory. **Your own keys are not part of the configuration** — see [You do not bring your own `nsec`](#you-do-not-bring-your-own-nsec).

### Required

| Variable | CLI flag | Description |
|---|---|---|
| `MOSTRO_PUBKEY` | `-m, --mostro-pubkey` | The `npub` (or hex) of the Mostro instance you want to use. |
| `RELAYS` | `-r, --relays` | Comma-separated `wss://` Nostr relay URLs. |

### Optional

| Variable | CLI flag | Description |
|---|---|---|
| `POW` | `-p, --pow` | The least proof of work (bits) mined on requests, default 0. The JSON-capable commands raise it to what the instance advertises, up to 20 bits, so you rarely need it; set it for a higher requirement or for the legacy commands. |
| `SECRET` | `-s, --secret` | Use secret/anonymous mode for the inner event tuple (advanced, hides trade index). |
| `TRANSPORT` | `-t, --transport` | Wire transport: only `nip44` (protocol v2, signed kind 14). Leave unset to auto-detect from the instance's info event (defaults to nip44). |
| `ADMIN_NSEC` | — | Admin/solver private key in `nsec1...` or hex format. Only read when an `adm*` command is invoked. |
| `MOSTRO_RPC_URL` | `http://127.0.0.1:50051` | `mostrod` admin gRPC endpoint (`[rpc]` in the daemon's settings). Only used by `admsetmaintenance` / `admmaintenancestatus` / `admcancelpending`. |
| `MOSTRO_RPC_TOKEN` | — | Bearer token for the admin gRPC, required when the daemon sets `[rpc].auth_token`. Only used by the three commands above. Sent in cleartext only to a loopback URL (direct or through an SSH tunnel); any other `http://` host is refused, use `https://` via a TLS proxy instead. |
| `RUST_LOG` | `-v, --verbose` | **Not actually configurable.** The logger is initialised only when `-v` is passed, and `-v` overwrites `RUST_LOG` with `info` first. So `RUST_LOG` alone produces no output, and `RUST_LOG=debug -v` still logs at `info`. `-v` is the only available level. |

`MOSTRO_PUBKEY`, `RELAYS`, `POW` and `TRANSPORT` may also be set in the [config file](#config-file); the other variables are read from the environment only.

### Choosing a Mostro instance

`mostro-cli` is only a client: it does not ship with a default node, and this README deliberately does not hard-code one. Mostro is a federation of independently operated daemons — any given instance can go offline, change its pubkey, or stop serving your currency at any time, so a pubkey pasted here would eventually send you to a dead node.

**Use the pubkey of the Mostro node you trust.** Ways to get one:

- **Ask the operator.** If you already trade on a given instance (through a mobile client, a community, or a friend), ask for its `npub` and the relays it publishes to.
- **Discover instances on Nostr.** Every running daemon publishes an addressable info event of **kind `38385`**, tagged with its `mostro_version`, `protocol_version`, `fee`, `pow` and `max_order_amount`. Querying a relay for that kind lists the instances it knows about, and the event's author pubkey is the value you need for `MOSTRO_PUBKEY`. Orders themselves are kind `38383` events authored by the same pubkey. Any Nostr client or CLI that can filter by kind will do.
- **Run your own.** The daemon is open source: [github.com/MostroP2P/mostro](https://github.com/MostroP2P/mostro). Running it yourself is also the recommended way to test the whole flow (including on testnet) without touching a stranger's node — its config file holds the pubkey and relays you then feed to `mostro-cli`.

Whichever you pick, `RELAYS` must include at least one relay that the instance actually publishes to, otherwise the CLI connects successfully and simply sees nothing.

### Suggested setup

Write the node settings once into the [config file](#config-file), so every new shell finds them, and check what it holds:

```bash
mostro-cli config set mostro-pubkey <npub-of-your-mostro-node>
mostro-cli config set relays wss://<relay-your-node-publishes-to>,wss://<another-relay>
mostro-cli config show
mostro-cli order list
```

Or write the file by hand (`mkdir -m 700`, `chmod 600`, as shown there), or create a small env file you `source` before using the CLI; it can also hold `ADMIN_NSEC`, which the config file refuses:

```bash
# ~/.config/mostro/env.sh   (chmod 600)
export MOSTRO_PUBKEY="<npub-of-your-mostro-node>"
export RELAYS="wss://<relay-your-node-publishes-to>,wss://<another-relay>"
# export POW=24                # only for more than 20 bits, or the legacy commands
# export ADMIN_NSEC=nsec1...   # only if you're an admin/solver
```

```bash
source ~/.config/mostro/env.sh
mostro-cli order list
```

> Replace both placeholders with the real values of the instance you want to trade on — see [Choosing a Mostro instance](#choosing-a-mostro-instance). No networked command works until `MOSTRO_PUBKEY` points at a live node (`--version`, `--help`, the offline `schema` and the local `identity init`, `identity show`, `order mine`, `config set` and `config show` are the only exceptions).

### Config file

Every command that talks to a node (`order list`, `order new`, `message list`, `listdisputes`, `getdmuser`, the admin commands, ...; not the local `schema`, `identity init`, `identity show` and `order mine`, nor the admin RPC commands `admsetmaintenance`, `admmaintenancestatus` and `admcancelpending`) also reads the node settings from `config` in the data directory: `$MCLI_DIR/config`, else `~/.mcli/config` (`%USERPROFILE%\.mcli\config` on Windows). It holds `KEY=VALUE` lines with the variable names; `#` comment lines, blank lines, an `export` prefix and quotes are accepted, but a `#` after a value is part of the value. So the `MOSTRO_PUBKEY` and `RELAYS` lines of the `env.sh` above can be copied there as they are, a `POW` line without its trailing comment, and `ADMIN_NSEC` stays out (see below).

```bash
mkdir -m 700 -p ~/.mcli
cat > ~/.mcli/config <<'END'
MOSTRO_PUBKEY=<npub-of-your-mostro-node>
RELAYS=wss://<relay-your-node-publishes-to>,wss://<another-relay>
END
chmod 600 ~/.mcli/config
mostro-cli order list
```

- An option wins over an environment variable that is set (even empty), which wins over the file.
- Only `MOSTRO_PUBKEY`, `RELAYS`, `POW` and `TRANSPORT` are allowed. Any other key, `ADMIN_NSEC` included, makes the whole file invalid, so keep secrets in the environment. Every value is checked like an option.
- Reading it never creates `~/.mcli` or anything in it, and its values never enter the environment.
- `mostro-cli config set <key> <value>` writes one setting for you (`mostro-pubkey`, `relays`, `pow` or `transport`), checked like an option and never echoed when refused: it creates the data directory if needed, rewrites the file as `NAME=value` lines (comments are dropped) and leaves a file it refuses untouched. On Unix it makes the directory (0700) and the file (0600) owner-only; on Windows it sets no permissions of its own, so both keep the ACLs they inherit from the parent folder.
- `mostro-cli config show` prints the file's path and the values stored in it (`not set` when absent), read-only. It shows only the file: an option or a set variable still wins over what it shows.
- A refused file stops the command before anything else happens: a JSON-capable one prints `hint: <path>: line N ...` and fails with `configuration_invalid`, a human-only one prints `Invalid config file <path>: line N ...`.

### About `.env` files

`mostro-cli` does **not** load a `.env` file automatically — there is no dotenv support in the binary, so dropping a `.env` next to the executable has no effect. Older guides (and some issue comments) suggest it; that advice is outdated. The only file it reads settings from is the [config file](#config-file), and only the four public node settings; `ADMIN_NSEC` stays in the environment.

If you prefer keeping settings in a `.env`-style file, export them yourself before running the CLI:

```bash
# ~/.config/mostro/.env   (chmod 600)
MOSTRO_PUBKEY=<npub-of-your-mostro-node>
RELAYS=wss://<relay-your-node-publishes-to>
POW=0
```

```bash
set -a; source ~/.config/mostro/.env; set +a
mostro-cli order list
```

`set -a` marks every variable assigned by the file for export, so the CLI sees them; `set +a` turns that back off.

---

## Quick start

Export your configuration (or put it in the [config file](#config-file)) first — every command below fails immediately without it:

```bash
export MOSTRO_PUBKEY="<npub-of-your-mostro-node>"
export RELAYS="wss://<relay-your-node-publishes-to>"
```

```bash
# 1. List open orders (read-only: no identity or local database is touched)
mostro-cli order list

# 2. Filter by side, currency or status
mostro-cli order list --side sell --currency usd
mostro-cli order list --side buy --currency ves --status pending

# 3. Inspect details for specific orders
mostro-cli order info --order <uuid-1> --order <uuid-2>   # needs your local identity; creates nothing

# 4. Create your own order (sell 1000-10000 ARS at market price)
mostro-cli order new --side sell --currency ARS --fiat-amount 1000 --fiat-amount-max 10000 --payment-method "face to face"

# 5. Take someone else's sell order
mostro-cli order take-sell --order <order-id> --amount 500     # optional fiat amount for range orders

# 6. After the trade, fetch new DMs from Mostro
mostro-cli message list --since 60
```

On the very first run you will see something like:

```
Creating database file with orders table...
User created with pubkey: <your i0_pubkey>
```

Write down or back up the mnemonic before doing anything else — see [Backup, recovery and multi-device](#backup-recovery-and-multi-device).

### The CLI does not stay connected — you poll

This is the biggest difference from a mobile Mostro client. Each `mostro-cli` invocation connects to the relays, sends (or reads) what you asked for, prints the result and exits. It does **not** keep running to notify you when your counterpart acts.

So a trade is driven by you re-running `message list`:

```bash
mostro-cli message list --since 60     # everything Mostro sent you in the last 60 minutes
mostro-cli --output json message list --since 60   # the same for a script: read `next_step` and `payload`
```

Run it after every step where you are waiting on the other side — an order being taken, an invoice arriving, fiat being marked as sent, sats being released. `--since` defaults to 30 minutes; widen it if you have been away. When a message asks you for something, its "👉 Next:" line names the command with the order id filled in.

**Follow a trade.** `order watch` is the one command that stays connected: it replays the last 30 minutes (`--since`), then prints each new message from Mostro and each chat message of one order as it arrives, until the trade ends. `--timeout SECONDS` stops it earlier with a final line, and Ctrl-C stops it at any time. With `--output json` each call returns one document, at the first new records or empty after `--timeout` (30 s by default), with a `cursor`: pass it back with `--after` to get only newer records, so a script loops watch, act, watch.

```bash
mostro-cli order watch --order <order-id>
mostro-cli --output json order watch --order <order-id> --after <cursor>
```

Pending orders also expire (24 hours on a typical instance — the exact value is the `expiration_hours` tag of the node's info event). If nobody takes your order before then, it disappears from the orderbook and any locked sats are returned; you can pass `--expiration-days N` (or `--expires-in SECONDS`, 1 to 31536000) to `order new` to request a different window; the node lowers it to its own maximum.

---

## Trading: selling sats step by step

This is the seller flow when you create the order (maker, sell).

1. **Create the order**

   ```bash
   mostro-cli order new --side sell --currency usd --fiat-amount 100 --payment-method "wise,strike"
   ```

   - `-k sell` — you are selling sats.
   - `-c usd` — fiat currency code.
   - `-f 100` — fiat amount (use `-f 100-500` for a range order).
   - `-m "wise,strike"` — comma-separated payment methods.
   - `-a 0` — sats amount (`0` = market price at trade time).
   - `-p 2` — optional price premium percentage.
   - `--expiration-days N` — optional custom expiration; `--expires-in SECONDS` asks for seconds instead (the two cannot be combined).

2. **Mostro replies with a hold invoice.** Pay it with your Lightning wallet. Funds are locked, not transferred yet.

3. **Wait for a buyer to take the order.** Check messages:

   ```bash
   mostro-cli message list --since 60     # last 60 minutes
   ```

4. **Buyer adds an invoice (if they didn't include one when taking)** — Mostro forwards their invoice.

5. **Buyer marks fiat as sent.** You'll see a `fiat-sent` message via `message list`. **Confirm you actually received the fiat** outside the CLI before releasing.

6. **Release the hold invoice** so the buyer gets the sats:

   ```bash
   mostro-cli order release --order <order-id>
   ```

7. **Rate your counterpart.** After `hold-invoice-payment-settled`, a `rate` message has the "👉 Next:" line with the `order rate` command:

   ```bash
   mostro-cli order rate --order <order-id> --rating 5
   ```

If something goes wrong before release, you can `order cancel --order <order-id>` (a pending order is canceled at once; an active trade needs both parties to agree) or `dispute open`. See [Disputes](#disputes-as-a-user).

---

## Trading: buying sats step by step

Buyer-as-taker flow against an existing sell order:

1. **Find a sell order:**

   ```bash
   mostro-cli order list --side sell --currency usd
   ```

2. **Take it.** You can either provide a Lightning invoice for the trade amount, or omit it and add one later:

   ```bash
   mostro-cli order take-sell --order <order-id> --invoice lnbc...    # with invoice
   mostro-cli order take-sell --order <order-id>                      # without invoice
   ```

   For range orders, also pass `--amount <fiat_amount>`.

3. **If you didn't provide an invoice, add one when prompted:**

   ```bash
   mostro-cli order add-invoice --order <order-id> --invoice lnbc...
   ```

4. **Pay the seller in fiat** using the agreed payment method.

5. **Tell Mostro fiat is sent.** The `hold-invoice-payment-accepted` message in `message list` has the "👉 Next:" line `` send the fiat, then: `order fiat-sent --order <order-id>` ``:

   ```bash
   mostro-cli order fiat-sent --order <order-id>
   ```

6. **Wait for the seller to release.** Check `message list`. When they release, Mostro pays your invoice. If that payment fails, `message list` shows "🔁 Payment failed: the node will retry the payment up to N more times, every M seconds"; Mostro retries on its own and asks for a new invoice only if every attempt fails. If it says the node will not retry, send a new invoice with the `order add-invoice` command its "👉 Next:" line names.

7. **Rate the seller.** After `purchase-completed`, a `rate` message has the "👉 Next:" line with the `order rate` command:

   ```bash
   mostro-cli order rate --order <order-id> --rating 5
   ```

### Buying as a maker

If you want to *post* a buy order instead of taking one, use `order new --side buy`. You'll typically include a Lightning Address as the invoice (`--invoice your@walletofsatoshi.com`) so the seller knows where to pay you.

---

## Direct messages with your counterpart

`order chat` reads and sends the kind-14 chat of an order you hold. It finds the counterparty's trade key itself, from the order's row
or the node's messages (it is known once the escrow is locked), so you never pass a key:

```bash
# Read both sides of the chat for an order (oldest first; --since defaults to 7 days)
mostro-cli order chat --order <order-id>

# Send one message (no prompt); the conversation is shown with it
mostro-cli order chat --order <order-id> --message "hi, sending now"

# The same for a script: JSON requires --yes with --message
mostro-cli --output json order chat --order <order-id> --message "hi, sending now" --yes

# Read what Mostro sent you (read-only; `--order` limits it to one order)
mostro-cli message list --since 30
mostro-cli message list --order <order-id>
```

The legacy commands also need the order's row and take the counterparty's key. Use them when `order chat` answers
`counterparty_unknown` and you know that key:

```bash
# Get the conversation key for a counterpart (informational)
mostro-cli conversationkey -p <their-pubkey>

# Read both sides of the kind-14 chat for an order
mostro-cli getdmuser -p <their-pubkey> -o <order-id> --since 120

# Send a DM (uses the order's trade key)
mostro-cli senddm -p <their-pubkey> -o <order-id> -m "hi, sending now"

# Send a kind-14 chat message to a user
mostro-cli dmtouser -p <their-pubkey> -o <order-id> -m "hello"
```

---

## Disputes (as a user)

If your counterpart misbehaves (no fiat received, no release after fiat sent, etc.):

```bash
mostro-cli dispute open --order <order-id>
```

This puts the order in dispute. A solver will be assigned and contacts you over
the **dispute chat**, a channel of its own: it is derived from your trade key and
the solver's pubkey, the same way the peer chat is derived from the two trade
keys. `message list` will not show it, because that command filters on events tagged
with your own pubkey while this conversation is tagged to its conversation key.

Read it, and answer, with:

```bash
mostro-cli disputechat -o <order-id>
mostro-cli disputechat -o <order-id> -m "here is the bank statement"
```

The solver's pubkey arrives in the `admin-took-dispute` message, so read it with `message list`
after opening the dispute (the `peer` payload carries the solver's pubkey; `message list` stores nothing) and use it with
`getdmuser --pubkey <solver> --order-id <id>` and `dmtouser`.

Be honest, provide evidence, and respect that the solver decides.

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

**If you have trades in flight, copy `~/.mcli/mcli.db` to the new machine.** This is the only path that lets you *continue* those trades, because commands like `order release`, `order cancel`, `order add-invoice` and `senddm` look the order up in the local `orders` table and fail without it. The file contains no funds — only your mnemonic and cached order metadata — but it does contain the mnemonic, so move it over a secure channel and keep the `0600` permissions.

### Restoring from the mnemonic alone

Use this when the database is gone. It recovers your **identity**, not your local order history — read the limitation at the end of this section before relying on it mid-trade.

> **Let the CLI create the database — do not hand-craft it.** `mostro-cli` only creates its tables when `~/.mcli/mcli.db` does not yet exist. If you pre-create that file yourself with just a `users` table, the `orders` table is never created, a read-only command such as `getadmindm` still appears to work, and the first command that touches an order fails with `no such table: orders`. Always run the CLI once first, then overwrite the mnemonic.

1. Install `mostro-cli` on the new machine.

2. **Configure the CLI first.** `MOSTRO_PUBKEY` and `RELAYS` are validated *before* the database is created, so without them the next step aborts and no database appears:

   ```bash
   export MOSTRO_PUBKEY="<npub-of-your-mostro-node>"
   export RELAYS="wss://<relay-your-node-publishes-to>"
   ```

3. **Run any command once** so the CLI builds a complete, correctly-permissioned database. It will generate a throwaway mnemonic that you are about to replace:

   ```bash
   mostro-cli getadmindm   # read-only on the network; any command that needs your identity creates the database
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

   Trade keys are derived from an incrementing index, and the daemon rejects an index it has already seen. A freshly restored database starts back at index 1, so without this sync your next order is refused. The command asks Mostro for your real last index and writes it back to the local database; it only ever raises the local index (the node does not count the keys of range children, so a lower answer is ignored).

6. **Ask Mostro what you have open:**

   ```bash
   mostro-cli restore
   ```

   This prints the identity pubkey it derived from your restored mnemonic, plus the ID, trade index and status of every active order and dispute Mostro holds for you.

7. **(Cosmetic) realign `i0_pubkey`.** The `users` row still carries the throwaway identity in its primary key column. Nothing derives from it — every identity and trade key comes from the `mnemonic` column at runtime, and `User::save` matches on whatever value is stored, so the database stays self-consistent. If you want the column to reflect reality anyway, take the `User` pubkey that `restore` printed in the previous step:

   ```bash
   sqlite3 ~/.mcli/mcli.db "UPDATE users SET i0_pubkey = '<pubkey printed by restore>';"
   ```

> **Limitation: `restore` does not rebuild your local order cache.** It reports what Mostro knows, but it does not insert those orders into the local `orders` table. Commands that operate on a specific order — `order release`, `order cancel`, `order fiat-sent`, `order add-invoice`, `order rate`, `dispute open`, `senddm` — read that table first and will fail on an order that is not in it. So a mnemonic-only restore gets your identity and your ratings back and lets you trade again from scratch, but it cannot resume a trade that was already in flight. For that, copy the database (see above).

> A friendlier `import-mnemonic` subcommand may land in the future. Until then, the flow above is the supported path.

---

## Command reference

Every command supports `-h, --help`. The list below is a one-line summary; run `mostro-cli <cmd> --help` for full flags.

### Order browsing & creation
- `order list [--side buy|sell] [--currency <code>] [--status <status>]` — list open orders (read-only; supports `--output json`). Replaces `listorders`.
- `schema` — describe the machine interface offline (supports `--output json`).
- `order info --order <uuid> [--order <uuid> ...]` — ask the node for its record of your own orders (supports `--output json`). Replaces `ordersinfo`.
- `order new --side <buy|sell> --currency <fiat> --fiat-amount <N> [--fiat-amount-max <M>] --payment-method <text> [--sats <N>] [--premium <P>] [--invoice <BOLT11>] [--expiration-days N | --expires-in SECONDS] [--yes]` — create an order (supports `--output json`, which requires `--yes`). Replaces `neworder`.

### Taking orders
- `order take-sell --order <id> [--invoice <invoice>] [--amount <fiat>] [--yes]` — buyer takes a sell order (supports `--output json`, which requires `--yes`). Replaces `takesell`.
- `order take-buy --order <id> [--amount <fiat>] [--yes]` — seller takes a buy order (supports `--output json`, which requires `--yes`). Replaces `takebuy`.
- `order add-invoice --order <id> --invoice <invoice> [--yes]` — buyer sends the invoice the node asked for after a take (supports `--output json`, which requires `--yes`). Replaces `addinvoice`; uses the order's stored key, spends no trade index.
- `--invoice -` (on `order add-invoice`, `bond add-invoice`, `order take-sell` and `order new`) reads the invoice from standard input instead of the command line, so it stays out of the process list: one line, at most 8 KiB, and then standard input must be closed (the program reads to the end); `--yes` is required in every mode (stdin cannot also answer the question), and a terminal as stdin is refused at once. Example: `printf '%s\n' "$INVOICE" | mostro-cli order add-invoice --order <id> --invoice - --yes`.

### Trade lifecycle
- `order fiat-sent --order <id> [--yes]` — buyer confirms fiat sent (supports `--output json`, which requires `--yes`). Replaces `fiatsent`; uses the order's stored key, spends no trade index.
- `order release --order <id> [--yes]` — seller releases the hold invoice; irreversible (supports `--output json`, which requires `--yes`). Replaces `release`. On a range order you made with room left, the maker's own step (`release` of a sell order, `fiat-sent` of a buy order) carries a fresh trade key so the node creates the child order for the rest of the range; the result gives the reserved `next_trade_index`, and `order sync` then gives the child a local row.
- `order sync [--since MINUTES]` — give the orders the node announced to your trade keys a local row: the child order of a range order, or any order the store lost (supports `--output json`; no `--yes`, it publishes nothing and never replaces a row).
- `order cancel --order <id> [--yes]` — cancel a pending order, or start or confirm a cooperative cancel of an active trade; the node decides which and the result says so (supports `--output json`, which requires `--yes`). Replaces `cancel`.
- `order rate --order <id> --rating <1-5> [--yes]` — rate the counterparty of a finished trade (supports `--output json`, which requires `--yes`). Replaces `rate`; writes nothing locally. A second rating gets no answer from the node and is `uncertain`.
- `dispute open --order <id> [--yes]` — open a dispute on an active or fiat-sent order (supports `--output json`, which requires `--yes`). Replaces `dispute`; the result carries the dispute id and the local status becomes `dispute`. A second dispute gets no answer and is `uncertain`.
- `order mine` — the orders of the local store, newest first, with their trade index and key; read-only, no node or relay (supports `--output json`).
- `bond add-invoice --order <id> --invoice <bolt11> [--yes]` — reply to a bond payout request with an invoice for your share of a slashed bond (supports `--output json`, which requires `--yes`; BOLT-11 only). Replaces `addbondinvoice`; without a confirmation from the node the result is `uncertain`, not success.

### Messaging
- `message list [--since <min>] [--order <id>]` — what Mostro sent you; read-only (replaces `getdm`; peer chat is `order chat`).
- `order chat --order <id> [--message <text>] [--since <min>]` — read the kind-14 chat of an order you hold and send one message, without passing the counterparty's key (supports `--output json`, which requires `--yes` with `--message`; human mode sends without a prompt). It replaces `getdmuser` and `dmtouser`.
- `getdmuser -p <pubkey> -o <id> [--since <min>]` — both sides of that order's kind-14 chat; needs the order's row. The fallback when `order chat` answers `counterparty_unknown` and you know the key.
- `senddm -p <pubkey> -o <id> -m <message>` — DM your counterpart.
- `dmtouser -p <pubkey> -o <id> -m <message>` — kind-14 peer chat; needs the order's row. The same fallback as `getdmuser`.
- `disputechat -o <id> [-s <min>] [-m <message>]` — read or answer the solver on the dispute chat.
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
- `admcancelpending -o <id>` — cancel a still-pending order, releasing its bonds; the daemon refuses any other status. Needs `mostrod` ≥ 0.18.7 (checked via `GetVersion` first; older daemons are refused).

### Solver tooling (no `ADMIN_NSEC` needed)
- `sendadmindmattach -p <pubkey> -o <id> -f <file>` — send an encrypted file attachment (uploaded to a Blossom server) over the order's trade key.

### Configuration
- `config set <mostro-pubkey|relays|pow|transport> <value>` — write one node setting to the [config file](#config-file), keeping the others; no node or relay (supports `--output json`).
- `config show` — the config file's path and the node settings stored in it, read-only (supports `--output json`).

### Identity / recovery
- `identity init` — create the local identity store (`MCLI_DIR`, else `~/.mcli`) with a fresh mnemonic, exactly as the first run of a legacy command does; no node, relay or `--yes`, and the mnemonic is never shown (supports `--output json`). Fails with `identity_exists` if a store is already there.
- `identity show` — the identity public key and last trade index of the local store, read-only (supports `--output json`).
- `restore` — re-sync active orders and disputes from Mostro.
- `getlasttradeindex` — fetch your last known trade index from Mostro.
- `getlasttradeprivkey` — show the private key for the last trade index (advanced).
- `getuserinfo` — show the reputation (rating, reviews, first trade) this Mostro node holds for **your** local identity (no user-npub arg). If first-contact PoW is required, the CLI fails fast with a `--pow` / `POW=` hint, or pass `--pow <pow_first_contact>` up front.

### Global flags

> **These must come *before* the subcommand.** They are parsed on the top-level command, so `mostro-cli order list -m <npub>` fails with `error: unexpected argument '-m' found`. Write `mostro-cli -m <npub> order list` instead. This also avoids clashing with subcommand flags that reuse the same letters (`-m` is `--message` on `senddm`, `-p` is `--pubkey` on the DM commands).

- `-v, --verbose` — enable info logging. This is the only log control; it overwrites `RUST_LOG` with `info`.
- `-m, --mostro-pubkey <npub>` — overrides `MOSTRO_PUBKEY` (the old spelling `--mostropubkey` was removed).
- `--reply-timeout <SECONDS>` — how long the commands that wait for the node's reply (`order new`, the takes, `order add-invoice`, the trade steps, `order info`, `dispute open`, `order rate`, `bond add-invoice`) wait for it: 5 to 300, default 15. Like `--output`, it may also follow the command words. Raise it for a slow node instead of repeating a write that came back `uncertain`.
- `--output <human|json>` — `json` is supported by `order list`, `schema`, `order info`, `order new`, `order take-sell`, `order take-buy`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order sync`, `order mine`, `order rate`, `dispute open`, `order chat`, `order watch`, `bond add-invoice`, `message list`, `identity init`, `identity show`, `config set` and `config show` only; see [Machine-readable output](#machine-readable-output).
- `-r, --relays <list>` — overrides `RELAYS`.
- `-p, --pow <bits>` — overrides `POW`.
- `-s, --secret` — secret mode for inner event tuple.
- `-t, --transport <nip44>` — overrides `TRANSPORT` (auto-detected when unset; only nip44 is supported).

`-m`, `-r`, `-p` and `-t` also override the [config file](#config-file): the file fills in a setting only when neither an option nor a variable that is set (even empty) gives it, and the transport is auto-detected when the `TRANSPORT` that wins is absent or empty.

```bash
mostro-cli -m <npub> -r wss://<relay> order list --side sell --currency usd
```

---

## Machine-readable output

Twenty-two commands can answer in JSON so that scripts and agents do not have to parse tables: `order list`, `schema`, `message list`, `identity init`, `identity show`, `order mine`, `config set`, `config show`, `order info`, `order sync`, `order new`, `order take-sell`, `order take-buy`, `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `order rate`, `dispute open` and `bond add-invoice` (the last ten need `--yes` and can report `uncertain`), `order chat` (`--yes` and `uncertain` only with `--message`), and `order watch` (one document per call, with a cursor for `--after`). Every other command is human-only for now, and asking it for JSON fails safely (exit 2, `output_not_supported`, nothing executed).

```bash
# one JSON document on stdout; diagnostics, if any, go to stderr
mostro-cli -m <npub> -r wss://<relay> --output json order list --side sell --currency usd

# what this build supports, without a node, identity or network
mostro-cli schema                  # pretty JSON
mostro-cli --output json schema    # the same data in the standard outcome
```

- `--output human|json` (default `human`, whether or not stdout is a terminal) may be written before, between or after the command words.
- An empty order book is a success with an empty list. A relay that fails, disconnects or stays silent is an error with a stable code, never an empty success; the read is bounded and never retried.
- Amounts and the event time are decimal strings, so 64-bit values survive any JSON consumer. `created_at` is the selected event's UTC time, or `null` if it is not representable.
- Exit codes: `0` success, `2` invalid arguments or unsupported JSON, `1` configuration, relay or internal errors. `--help` and `--version` always print text and exit 0.
- The [mostro-cli book](https://mostro.network/mostro-cli-book/) documents each of these commands with its fields, codes and examples, the trade flows and a [guide for AI agents](https://mostro.network/mostro-cli-book/wrapper/ai_agents.html).
- The generated command reference is [docs/schemas/discovery-v1.json](docs/schemas/discovery-v1.json) and the listing result schema is [docs/schemas/order-list-v1.json](docs/schemas/order-list-v1.json). Field, limit, error and versioning details are in [docs/commands.md](docs/commands.md#machine-output-schema-v1). The output schema version is independent of the CLI version. Two enums of the message records, `next_step` and the payload `kind`, are open within version 1: treat an unknown `next_step` as `null` and a payload of an unknown `kind` as opaque (see the versioning rule in docs/commands.md).

**Migrating from `listorders`.** `listorders`, its `-k`, `-c` and `-s` flags and the root `--mostropubkey` spelling were removed without aliases. Use `order list --side <buy|sell> --currency <code> --status <status>` and `--mostro-pubkey` (or `-m`) before the command word.

> **Using `mostro-mcp`?** The existing MCP server still runs `listorders --kind ...` and the other old spellings, and it does not read this JSON yet. Keep running it against the CLI revision its own README names until it is migrated; do not point it at a build of this CLI that has removed those spellings. No compatible CLI/MCP release pair is published yet.

---

## Files, environment and where things live

| Path | What it is |
|---|---|
| `~/.mcli/` | The CLI's data directory. Created on first run. |
| `~/.mcli/config` | Optional node settings as `KEY=VALUE` lines (see [Config file](#config-file)); never created by a read. `config set` writes it, with its lock `.config.lock` beside it. |
| `~/.mcli/mcli.db` | SQLite database with your `users` row (mnemonic, identity key, last trade index) and `orders` cache. |

Environment variables read by the CLI:

| Var | Purpose |
|---|---|
| `MOSTRO_PUBKEY` | Required — Mostro instance pubkey. |
| `RELAYS` | Required — Nostr relays. |
| `POW` | Optional — proof-of-work bits. |
| `SECRET` | Optional — `true` enables secret-mode inner tuple. |
| `TRANSPORT` | Optional — `nip44` only; auto-detected when unset. |
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

**`MOSTRO_PUBKEY not set`** — Export it, put it in the [config file](#config-file) (`mostro-cli config set mostro-pubkey <npub>`; `mostro-cli config show` lists what the file holds), or pass `-m <npub>` **before** the subcommand (`mostro-cli -m <npub> order list`, not `mostro-cli order list -m <npub>` — see [Global flags](#global-flags)). Mind the underscore: only `MOSTRO_PUBKEY` is read, `MOSTROPUBKEY` is not. If you don't have a node pubkey yet, see [Choosing a Mostro instance](#choosing-a-mostro-instance). The same message means the value that wins is blank: an empty `-m`, or a `MOSTRO_PUBKEY` that is set but empty, whatever the config file holds; unset the variable to use the file.

**`The node configuration is invalid.`** after a `hint: <path>: line N ...` line, or **`Invalid config file <path>: line N ...`** — The config file was refused. The hint names the file and the line: a key other than the four settings, a line without `=`, or a value that is not a key, relay URLs, a number up to 255 or `nip44`. Fix or remove that line (`mostro-cli config show` names it too); the whole file is refused even when the options give every value. Without a hint, the option or variable itself is invalid.

**`The configuration file cannot be written, so nothing was changed.`** (`configuration_unwritable`) — `config set` could not create the data directory, its lock or temporary file (on Unix a `.config.lock` that is a symlink is refused), or could not write or rename the file. The old file is intact, but the data directory may already have been created (or, on Unix, made owner-only, 0700), with an empty `.config.lock` in it. Make `MCLI_DIR` (else `~/.mcli`) a directory you own on a disk with free space, then run it again.

**`RELAYS not set`** — `RELAYS` is required too, and that exact name is the one the CLI reads. Export it (comma-separated `wss://` URLs), put it in the [config file](#config-file) (`mostro-cli config set relays <relay[,relay...]>`) or pass `-r <relay[,relay...]>` before the subcommand. As with the key, a blank `-r` or a `RELAYS` that is set but empty counts as missing, whatever the file holds.

**`error: unexpected argument '-m' found`** (or `-r`, `-p`, `-t`, `-v`) — Global flags belong before the subcommand: `mostro-cli -m <npub> order list`. Placed after it, clap parses them against the subcommand, which either rejects them or silently means something else. See [Global flags](#global-flags).

**My `.env` file is ignored** — It is not loaded automatically; the CLI has no dotenv support. Use `set -a; source .env; set +a` first — see [About `.env` files](#about-env-files).

**`ADMIN_NSEC not set (required for admin commands)`** — Only admin subcommands need it. Export it in the same shell, or prefix the command: `ADMIN_NSEC=nsec1... mostro-cli admsettle ...`.

**`order list` returns nothing** — Almost always a configuration problem rather than an empty orderbook. In order:

1. **Is `MOSTRO_PUBKEY` a live instance?** A node that has been shut down, or a pubkey copied from an outdated guide, produces exactly this: a clean connection and zero orders. Confirm the pubkey with its operator, or look for its kind-`38385` info event on the relay — see [Choosing a Mostro instance](#choosing-a-mostro-instance).
2. **Do your relays carry that instance?** The node only publishes to the relays it is configured with. A perfectly healthy relay that the node never writes to will show nothing.
3. **Is the relay reachable?** Test with e.g. `websocat wss://<your-relay>`.
4. **Are your filters too narrow?** `-k`, `-c` and `-s` combine; drop them and retry.

Run with `-v` (before the subcommand) for relay-level logs.

**Mostro rejects events / no reply** — The instance may require `POW`. The JSON-capable commands mine the advertised difficulty up to 20 bits and report `pow_required` with `required` and `configured` above that; export `POW=<required>` to mine it. For the legacy commands, ask the operator (or read the `pow` tag of its kind-38385 event) and export `POW=<bits>`.

**`no such table: orders`** — Your `~/.mcli/mcli.db` was created by something other than the CLI (usually by hand-crafting it while restoring a mnemonic). The CLI only creates its tables when that file does not exist, so a pre-made database is missing `orders`. Delete it and follow [Restoring from the mnemonic alone](#restoring-from-the-mnemonic-alone) — but back up the mnemonic first: `sqlite3 ~/.mcli/mcli.db "SELECT mnemonic FROM users;"`.

**Mostro rejects my order after restoring on a new machine** — You very likely skipped the trade-index sync. Run `mostro-cli getlasttradeindex`; see [Restoring from the mnemonic alone](#restoring-from-the-mnemonic-alone).

**Nothing happens / I'm waiting for my counterpart** — The CLI does not stay connected. Re-run `mostro-cli message list --since <minutes>` to pull new messages; see [The CLI does not stay connected — you poll](#the-cli-does-not-stay-connected--you-poll).

**Lost the database / changed machine** — See [Backup, recovery and multi-device](#backup-recovery-and-multi-device). Without the mnemonic you cannot recover anything; with the mnemonic you recover your identity but not in-flight trades (see below).

**I restored my mnemonic but `order release` / `order add-invoice` / `senddm` says the order doesn't exist** — Expected. `mostro-cli restore` reports the orders Mostro holds for you, but it does not write them into the local `orders` table, and those commands look the order up there first. A mnemonic-only restore cannot resume a trade that was already in flight — copying `~/.mcli/mcli.db` is the only way to do that. See [Moving to a new machine: copy the database](#moving-to-a-new-machine-copy-the-database).

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

Mostro CLI is licensed under the [GNU General Public License v3.0](LICENSE).

The project was previously distributed under the MIT License. That copyright
and permission notice is preserved in [LICENSE-MIT](LICENSE-MIT) for the
historical MIT-licensed work.
