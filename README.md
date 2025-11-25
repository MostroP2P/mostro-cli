# Mostro CLI 🧌

![Mostro-logo](static/logo.png)

Very simple command line interface that show all new replaceable events from [Mostro](https://github.com/MostroP2P/mostro)

## Requirements

0. You need Rust version 1.64 or higher to compile.
1. You will need a lightning network node

## Install dependencies

To compile on Ubuntu/Pop!\_OS, please install [cargo](https://www.rust-lang.org/tools/install), then run the following commands:

```bash
sudo apt update
sudo apt install -y cmake build-essential pkg-config
```

## Install

You can install directly from crates:

```bash
cargo install mostro-cli
```

Or downloading and compiling it by yourself:

```bash
git clone https://github.com/MostroP2P/mostro-cli.git
cd mostro-cli
# Edit .env-sample and set MOSTRO_PUBKEY, RELAYS, and POW
# For admin commands, also set ADMIN_NSEC
source .env-sample
cargo run
```

## Usage

```text
Commands:
  listorders         Requests open orders from Mostro pubkey
  neworder           Create a new buy/sell order on Mostro
  takesell           Take a sell order from a Mostro pubkey
  takebuy            Take a buy order from a Mostro pubkey
  addinvoice         Buyer add a new invoice to receive the payment
  getdm              Get the latest direct messages
  getadmindm         Get the latest direct messages for admin
  senddm             Send direct message to a user
  fiatsent           Send fiat sent message to confirm payment to other user
  release            Settle the hold invoice and pay to buyer
  cancel             Cancel a pending order
  rate               Rate counterpart after a successful trade
  restore            Restore session to recover all pending orders and disputes
  dispute            Start a dispute
  admcancel          Cancel an order (only admin)
  admsettle          Settle a seller's hold invoice (only admin)
  listdisputes       Requests open disputes from Mostro pubkey
  admaddsolver       Add a new dispute's solver (only admin)
  admtakedispute     Admin or solver take a Pending dispute (only admin)
  admsenddm          Send gift wrapped direct message to a user (only admin)
  conversationkey    Get the conversation key for direct messaging with a user
  getlasttradeindex  Get last trade index of user
  help               Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose
  -m, --mostropubkey <MOSTRO_PUBKEY>
  -r, --relays <RELAYS>
  -p, --pow <POW>
  -h, --help                         Print help
  -V, --version                      Print version
```

## Examples

```bash
$ mostro-cli -m npub1ykvsmrmw2hk7jgxgy64zr8tfkx4nnjhq9eyfxdlg3caha3ph0skq6jr3z0 -r 'wss://relay.mostro.network,wss://relay.damus.io' listorders

# You can set the env vars to avoid the -m, -n and -r flags
$ export MOSTRO_PUBKEY=npub1ykvsmrmw2hk7jgxgy64zr8tfkx4nnjhq9eyfxdlg3caha3ph0skq6jr3z0
$ export RELAYS='wss://relay.mostro.network,wss://relay.damus.io'
$ mostro-cli listorders

# Create a new buy order
$ mostro-cli neworder -k buy -c ves -f 1000 -m "face to face"

# Cancel a pending order
$ mostro-cli cancel -o eb5740f6-e584-46c5-953a-29bc3eb818f0

# Create a new sell range order with Proof or work difficulty of 10
$ mostro-cli neworder -p 10 -k sell -c ars -f 1000-10000 -m "face to face"
```

## Progress Overview

- [x] Displays order list
- [x] Take orders (Buy & Sell)
- [x] Posts Orders (Buy & Sell)
- [x] Sell flow
- [x] Buy flow
- [x] Maker cancel pending order
- [x] Cooperative cancellation
- [x] Buyer: add new invoice if payment fails
- [x] Rate users
- [x] Dispute flow (users)
- [x] Dispute management (for admins)
- [x] Create buy orders with LN address
- [x] Direct message with peers (use nip-17)
- [x] Conversation key management
- [x] Add a new dispute's solver (for admins)
- [x] Identity management (Nip-06 support)
- [x] List own orders
