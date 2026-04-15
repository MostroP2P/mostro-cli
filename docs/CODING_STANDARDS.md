# Coding Standards

This document outlines the coding standards and best practices for the Mostro CLI project. These guidelines ensure code quality, maintainability, and consistency across the codebase.

## Core Principles

### 1. Readability and Reuse

**Priority**: Code should be written for humans first, machines second.

- **Clear naming**: Use descriptive names for functions, variables, and modules (e.g., `parse_dm_events` vs `pde`).
- **Function reuse**: Extract common logic into reusable functions. Place shared utilities in appropriate modules (`src/util/`, `src/parser/`, etc.).
- **Module organization**: Group related functionality together (CLI commands in `src/cli/`, Protocol parsing in `src/parser/`, Utilities in `src/util/`).

### 2. Avoid Code Duplication (DRY Principle)

**Don't Repeat Yourself**: If the same logic appears in multiple places, extract it.

- **Extract common patterns**: Create helper functions for repeated operations like DM sending.
- **Centralize constants**: Import from `mostro-core::prelude` instead of hardcoding values.

### 3. Simplicity

**Keep It Simple**: Prefer straightforward solutions over clever ones.

- **Avoid premature optimization**: Write clear code first, optimize only when needed.
- **Prefer explicit over implicit**: Use `Option` and `Result` types explicitly rather than hiding errors with `unwrap()`.

### 4. Function Length Limit

**Maximum 300 lines per function**: If a function exceeds this limit, split it into smaller, single-responsibility functions.

## Rust-Specific Guidelines

### Error Handling

- **Use `Result<T, E>`**: Functions that can fail should return `Result`.
- **Use `anyhow::Result`**: For application-level errors, use `anyhow::Result<T>`.
- **Propagate errors**: Use the `?` operator to propagate errors up the call stack.
- **Add context**: Use `.context("...")` from `anyhow` to add meaningful error messages.

### Type Safety

- **Use strong types**: Prefer newtypes or enums (`Action`, `Status`) over primitive types.
- **Leverage enums**: Use enums for state machines and role management.

### Async/Await

- **Prefer async/await**: Use `async fn` for I/O and network operations.
- **Handle timeouts**: Use `tokio::time::timeout` for network operations.

### Documentation

- **Document public APIs**: Use `///` doc comments for public functions and types.
- **Explain "why"**: Document the reasoning behind complex logic, not just "what".
- **Markdown standards**: All markdown documentation must pass linter checks with no errors.
  - Run markdown linters to catch formatting issues (MD040, MD060, MD022, MD032, etc.).
  - Fix all linter errors before committing documentation changes.
  - Use proper table formatting with spaces around pipes.
  - Specify language for all fenced code blocks.
  - Add blank lines around headings and lists.

## Nostr and Mostro-Specific Guidelines

### Event Kinds

- **Use constants**: Always use constants from `mostro-core::prelude` (e.g., `NOSTR_ORDER_EVENT_KIND`).
- **Never hardcode**: Avoid hardcoding event kind numbers like 38383.

### Message Handling

- **Parse DMs consistently**: Use `parse_dm_events` for all DM parsing.
- **Support multiple message types**: Handle both GiftWrap (NIP-59) and PrivateDirectMessage (NIP-17).

### Key Management

- **Identity vs Trade Keys**:
  - **Identity keys** (index 0): User's main identity, used for signing.
  - **Trade keys** (index 1+): Ephemeral keys for each trade, ensuring privacy.

## Code Organization Patterns

### Module Structure

```text
src/
├── main.rs              # Entry point
├── cli.rs               # CLI definitions and Context
├── db.rs                # Database models (User, Order)
├── cli/                 # CLI command implementations
├── parser/              # Event parsing and display
└── util/                # Core utilities (events, messaging, net)
```

### Re-export Pattern

Use `mod.rs` files to re-export commonly used items from submodules to keep imports clean.

## Database Patterns

- **User Model**: Use chainable setters and the `.save()` pattern.
- **Order Management**: Use `Order::new()` to handle both insertion and updates.

## CLI Command Pattern

All CLI commands follow a standard flow:

1. Validate inputs.
2. Get required keys (Identity/Trade).
3. Build the Mostro message.
4. Send to Mostro (NIP-59).
5. Wait for response (Subscription).
6. Parse and display results.

## Summary Checklist

- [ ] Code is readable and well-named.
- [ ] No code duplication (DRY).
- [ ] Functions are under 300 lines.
- [ ] Errors are handled properly (`Result`, `?`).
- [ ] Event kinds use constants from `mostro-core`.
- [ ] Both GiftWrap and PrivateDirectMessage are supported.
- [ ] Public APIs are documented.
- [ ] All markdown documentation passes linter checks with no errors.
- [ ] Code passes `cargo fmt` and `cargo clippy`.
- [ ] Tests pass (`cargo test`).
