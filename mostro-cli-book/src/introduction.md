# Introduction

`mostro-cli` is a command-line client for [Mostro](https://mostro.network), the peer-to-peer
Lightning ⇄ fiat exchange that runs over Nostr. Besides the human output it has always had,
a growing set of its commands can print **one machine-readable JSON document** per run:

```sh
mostro-cli --output json order list --currency USD
```

```json
{"schema_version":"1","command":"order.list","status":"ok","data":{...},"error":null,"warnings":[],"meta":{...}}
```

This book is for whoever writes the program on the other side of that pipe: a generic
wrapper, a GUI, a bot, an MCP server, or an AI agent. It explains:

- **every command** that supports JSON, with its options and what it does on the network
  and on disk;
- **every field** of every document, every status, error code, warning code and
  execution stage, and how to react to each;
- **many examples**: 120 outcomes, each in its own file with the exact command line, all
  validated against the schemas the CLI generates.

## How this relates to the Mostro protocol book

The [protocol book](https://mostro.network/protocol/) describes the Nostr messages
exchanged between a client and the Mostro daemon (`new-order`, `take-sell`, `add-invoice`,
`fiat-sent`, `release` ...). `mostro-cli` speaks that protocol for you: it holds the keys,
signs and encrypts requests, waits for the correlated reply and stores the order locally.
What you see is the **result**, already decoded, typed and checked.

| Protocol book | This book |
| --- | --- |
| Nostr events, NIP-44 kind-14 messages | Process arguments, stdout, exit code |
| `action` + `payload` of a message | `command` + `data` of an outcome |
| `cant-do` with a reason | `error.code = "node_refused"` + `error.reason` |
| Keys, trade indexes, signatures | Handled by the CLI; only public keys and indexes are reported |

## The contract in five lines

1. Pass `--output json`; write commands also need `--yes`. Nothing is ever read from stdin.
2. Stdout holds exactly one JSON object and a newline. Diagnostics go to stderr.
3. Branch on `status` (`ok`, `error`, `uncertain`), then on `error.code`. Never on `message`.
4. `uncertain` means *the node may have acted*. Do not retry blindly; check first.
5. Every key is always present; empty values are `null` or `[]`, never omitted.

## Versions

| Item | Value |
| --- | --- |
| JSON contract | `schema_version` `"1"` |
| Documented executable | mostro-cli 0.16.3 (`develop`, 2026-11-02) |
| Schema dialect | JSON Schema Draft 2020-12 |
| Node protocol | Mostro protocol v2 (NIP-44 kind-14); v1 nodes are refused |

The contract version is independent of the executable version. Within version 1 only
optional additions are allowed (a new command, a new code scoped to a new command). A
renamed command, a changed type or a changed meaning requires `schema_version` `"2"`.
Reject documents whose `schema_version` you do not know.

> `order sync` is documented from the development branch, where it is implemented and
> tested but not yet released. Check `mostro-cli --output json schema` on your binary.
