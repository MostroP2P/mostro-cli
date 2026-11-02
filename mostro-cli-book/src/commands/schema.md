# schema

| | |
| --- | --- |
| `command` | `schema` |
| Kind | Offline discovery |
| Needs | Nothing: no configuration, identity, network or stdin |
| Statuses | `ok`, `error` |
| Schemas | `schemas/schema.input.json`, `schemas/schema.output.json` |

Describes the machine interface of **this** binary: every command, whether it supports
JSON, its input schema, its output schema and the configuration it reads.

## Invocation

```text
mostro-cli --output json schema
mostro-cli schema                  # human mode: the same DiscoveryData, pretty-printed, without envelope
```

## Input

None. Any option is `invalid_arguments`.

## Behavior

Pure and deterministic for a given binary. It never reads environment values (so it never
prints secrets or relay URLs), never opens the database and never connects. Call it once
at wrapper start-up and cache it keyed by `executable_version`.

## Success data: `DiscoveryData`

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | string | Contract version, `"1"` |
| `executable_version` | string | Package version of the binary, e.g. `"0.16.3"` |
| `schema_dialect` | string | `"https://json-schema.org/draft/2020-12/schema"` |
| `commands` | array of `CommandDescriptor` | Sorted by `name`; JSON-capable and human-only commands alike |

### CommandDescriptor

| Field | Type | Meaning |
| --- | --- | --- |
| `name` | string | Canonical dotted name, as in the `command` field of outcomes |
| `cli_path` | array of string | Tokens to type today, e.g. `["order", "take-sell"]`, `["getdmuser"]` |
| `output_modes` | array of `"human"` \| `"json"` | `["human", "json"]` for migrated commands, `["human"]` for legacy ones |
| `input_schema` | object or null | JSON Schema of the options; `null` for human-only commands |
| `output_schema` | object or null | JSON Schema of the whole envelope; `null` for human-only commands |
| `required_configuration` | array of string or null | Environment variables read (`["MOSTRO_PUBKEY", "RELAYS"]`); `[]` for `schema`; `null` when not modeled (legacy) |

Example descriptor (trimmed):

```json
{
  "name": "order.cancel",
  "cli_path": ["order", "cancel"],
  "output_modes": ["human", "json"],
  "input_schema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "order": { "type": "string", "description": "Id of the order" },
      "yes": { "type": "string", "enum": ["true", "false"], "description": "Send the request without asking (required with --output json)" }
    },
    "required": ["order"]
  },
  "output_schema": { "title": "...", "oneOf": ["..."], "$defs": {"...": "..."} },
  "required_configuration": ["MOSTRO_PUBKEY", "RELAYS"]
}
```

Input schemas use one extension keyword: `"x-case-insensitive": true` on enums that accept
any letter case (`side`, `status` of `order list`).

## Outcomes

| Case | status | code | Stages |
| --- | --- | --- | --- |
| Described | `ok` | | `not_applicable` ×3 |
| An option was given | `error` | `invalid_arguments` (exit 2) | `not_applicable` ×3 |
| Could not serialize | `error` | `internal_error` | `not_applicable` ×3 |

## Using it in a wrapper

```sh
# names of JSON commands and the codes each can fail with
mostro-cli --output json schema | jq -r '
  .data.commands[] | select(.output_schema != null) |
  "\(.name): \([.output_schema.oneOf[] | .properties.error.properties.code.enum? // empty] | add | unique | join(", "))"'
```

Check at start-up that every command your wrapper uses is present with `"json"` in
`output_modes`; refuse to run against an older binary instead of failing mid-trade.

## Examples

{{#include ../generated/schema.md}}

## Next

[Command index](README.md) · [order list](order_list.md)
