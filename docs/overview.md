## mostro-cli Overview

`mostro-cli` is a command-line client for interacting with the Mostro P2P Bitcoin/fiat marketplace. It talks to the Mostro backend over Nostr, manages a local SQLite database of orders and user identity, and wraps Mostro protocol flows into ergonomic commands.

The CLI is heavily inspired by the Mostro backend documentation (`mostro/docs` in the main repo), but this documentation is specific to the CLI and is meant to give humans and AI assistants enough context to safely extend the tool.

### High-level responsibilities

- **Order lifecycle**: create, take, cancel, dispute, and settle orders using `mostro_core` types and the Mostro protocol.
- **Direct messaging**: send and receive Nostr DMs between users, admins, and solvers, including gift-wrapped messages, shared-key custom wraps (ECDH-derived key, NIP-44 inside NIP-59) for `dmtouser` and admin attachment DMs, and encrypted attachments.
- **Local persistence**: keep a local cache of orders and a deterministic identity in a SQLite database (`mcli.db`) under the CLI data directory.
- **Admin / solver tooling**: expose admin-only and solver-only flows (e.g. taking disputes, adding solvers, admin DMs) when run with the proper keys.

### Key crates and technologies

- **Rust + Tokio**: async CLI built on the Rust ecosystem.
- **clap**: command-line parsing, subcommands, help text, and argument validation.
- **nostr-sdk**: Nostr client for relay connectivity, DMs, and events.
- **mostro_core**: shared protocol types for orders, disputes, and messages.
- **sqlx + SQLite**: local storage for users and orders (`mcli.db`).

### Core modules (top-level)

- **`src/main.rs`**: entrypoint; wires `mostro-cli` to the `cli::run()` async function.
- **`src/cli.rs`**: defines the `Cli` struct, the `Commands` enum (all subcommands), context initialization, and the main dispatch logic for commands.
- **`src/db.rs`**: database connection and schema management, plus `User` and `Order` models and helpers.
- **`src/util/*`**: shared utilities for events, Nostr networking, messaging, storage helpers, and type wrappers.
- **`src/parser/*`**: parsing helpers for events (orders, disputes, DMs) into higher-level types.
- **`src/lightning/*`**: Lightning-related helpers used by invoice / payment flows.

### How to read this docs folder

This docs folder is optimized for AI-assisted development:

- **`commands.md`**: One-stop reference for all CLI commands, arguments, and their handler functions. Useful when adding, renaming, or refactoring commands.
- **`architecture.md`**: Overview of module structure and main data / control flows.
- **`database.md`**: SQLite schema (`orders`, `users`), how they are used, and migration notes.

If you add new subcommands, modules, or tables, please also update the relevant markdown file so that future contributors (and AI tools) have an up-to-date view of the system.

