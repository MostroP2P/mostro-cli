# CLI Contract: Order Listing And Discovery

**Status**: Proposed schema v1 interface. The current binary does not implement it.

[Data model](../data-model.md) defines wire fields; [examples](examples.json)
are synthetic design fixtures, not captured production output or generated schemas.

## Invocation

```text
mostro-cli [ROOT OPTIONS] order list [--side buy|sell] [--currency TEXT] [--status STATUS]
mostro-cli [ROOT OPTIONS] schema
```

Root configuration: `--mostro-pubkey PUBLIC_KEY` overrides `MOSTRO_PUBKEY` and
`--relays URL[,URL...]` overrides `RELAYS`. Place existing root options, including
`--verbose`, before the command. Listing needs those two configuration values;
schema needs neither. Empty/invalid supplied CLI configuration overrides, rather
than falling back to, environment values. Parse public keys with the existing
SDK parser, allowing its existing hex/npub forms; emit canonical hex in results.

`--output human|json` is global, case-sensitive, optional and defaults to human.
It accepts `--output=json`, before, between or after command path components.
It is not discovered inside another argument's value or after `--`. Reject
duplicate output declarations. Side/status input is case-insensitive; see the
data model for defaults, exact accepted statuses and currency normalization.

Only `listorders` and its `--kind` spelling are replaced by `order list --side`
in this slice. The root `--mostropubkey` spelling is replaced by
`--mostro-pubkey`. Removed spellings fail before effects; no aliases/adapters.
Other legacy command invocations stay human-only until their own migration.
No new `--yes`, `--reveal`, write command or all-statuses option is introduced.

For equivalent valid list invocations, all these select JSON:

```text
mostro-cli --output json order list
mostro-cli order --output=json list
mostro-cli order list --output json
```

## Streams And Exit Codes

With JSON selected, a controlled outcome is exactly one UTF-8 JSON object followed
by newline on stdout. No progress line, ANSI sequence, table or diagnostic may
precede/follow it. Stderr may contain safe diagnostics only, including verbose.
No prompt or stdin read in listing, discovery or unsupported-JSON handling.

| Outcome | Exit | Stdout |
| --- | --- | --- |
| Successful listing, including empty | 0 | ok envelope; orders is always an array |
| Successful schema with JSON selected | 0 | ok envelope whose data is DiscoveryData |
| Argument/unsupported-output error | 2 | error envelope when JSON intent is unambiguous |
| Configuration/read/internal runtime error | 1 | error envelope when possible before stdout write |
| Valid help/version request | 0 | Conventional text even when JSON was selected |
| Invalid/missing/duplicate output selection | 2 | Empty; safe stderr diagnostic |
| Ambiguous output recovery on malformed argv | 2 | Empty; safe stderr diagnostic |
| Serialization failure before writing | 1 | Safe internal_error envelope if it can be encoded, otherwise empty with stderr diagnostic |
| Broken/partial stdout write | 1 | May be incomplete; never append another envelope |
| Crash/external kill | OS-dependent | No final-document guarantee |

Clap decides help/version precedence; only its actual display-help/version result
gets the text exception. No tokens are removed or reparsed into an executable
command by bootstrap recovery. In human mode failures remain nonzero and use safe
stderr. No TTY-based output switching.

Human listing preserves familiar fields, empty-book wording and readable tables.
Human schema presentation is pretty-printed raw DiscoveryData, as appropriate for
schema inspection; machine consumers select JSON for the standard envelope.
Schema does not parse node configuration, initialize logging from secrets, create
identity state, connect, perform RPC or read stdin in either presentation.

## Stable Errors

Messages are safe fixed prose; only code is stable for matching. Never serialize
raw Clap/SDK/anyhow text, command arguments, credentials, relay URLs or server text.

| Code | Exit | Condition |
| --- | --- | --- |
| invalid_arguments | 2 | Unknown/missing/invalid argument, side or status, or removed spelling |
| output_not_supported | 2 | Valid invocation of a registered unmigrated command requesting JSON |
| configuration_missing | 1 | Required public key or relays missing/empty |
| configuration_invalid | 1 | Invalid public key or relay URL |
| relay_connection_failed | 1 | A configured relay cannot connect within the connection budget |
| relay_read_failed | 1 | REQ rejected/CLOSED, pre-EOSE disconnect, notification lag or other known read failure |
| read_timeout | 1 | Collection deadline reached without all required EOSE |
| read_limit_exceeded | 1 | More than 10,000 unique matching events before completion |
| internal_error | 1 | Unexpected local invariant/serialization failure, without raw details |

Malformed legacy invocation is invalid_arguments, not output_not_supported.
Valid unsupported invocation fails before environment mutation, RPC or Context.
Use the resolved canonical command name from the coverage registry, e.g.
order.new for parsed neworder; use null when the path cannot be resolved safely.
Do not derive identity from arguments that resemble commands. Known order.list
errors retain order.list. Stage metadata follows the data model even on errors.

No pilot path returns uncertain or automatically retries. In particular, a
timeout with partial events is error/data null, not an empty or partial success.

## Snapshot Contract

Query kind 38383, author = configured public key, `#z=order`, since seven days
before query creation, no until, limit 50 per relay. The adapter verifies/filter-
matches events and deduplicates by event ID. Reduce latest state by UUID before
side/currency/status filtering. Preserve permissive tag decoding and specified
tie behavior from the data model.

Arm notifications before subscriptions; require EOSE from all deduplicated
configured relays. Freeze each relay at first EOSE. Any incomplete/failed relay
makes the whole read fail without partial data. Bound connection/read/cleanup
at 10/15/1 seconds respectively; no automatic resubmission after a failed query.
EOSE is evidence of bounded query completion, not an exhaustive book guarantee.
An empty valid snapshot is success even if filtering removed every event.

No trade EVENT publication or identity/DB access is allowed. NIP-42 AUTH using
ephemeral keys and REQ/CLOSE protocol traffic are allowed; no DM probe is needed.

## Discovery And Versioning

Bare schema and schema with JSON selected describe both machine commands and
mark legacy entries human-only. Their descriptors include actual invocation
paths, canonical names, generated input/output schemas and configuration names.
Do not publish future command paths as executable coverage. Only declared static
defaults may appear; never materialize environment values or credentials.

Generate Draft 2020-12 schema shapes from owned DTOs, not core/database structs.
Generate input metadata from Clap; share status/default/normalization validation
and test semantic rules that JSON Schema cannot express. Preserve decimal-string
patterns, nullable versus required fields and status/data/error constraints.

During implementation, check generated command/schema fixtures into the existing
documentation tree and compare generation in CI. The proposed examples here test
semantic conformance, not byte-for-byte identity with generated schema formatting.
Do not call these hand-authored examples source-generated output.

Contract version `1` is independent of CLI package version. Consumers reject
unsupported schema versions and incompatible command coverage. Only optional
additions are compatible within v1; incompatible enum/type/semantic changes require
a new version. Pilot command schemas allow ok/error only, while the shared
envelope reserves uncertain for future writes.

The existing MCP is not compatible with the removed listorders spelling. Keep
deployed MCP paired with its current CLI until its separately tested JSON upgrade;
this plan does not authorize replacing its executable or assigning release versions.
