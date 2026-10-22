# Data Model: Order Listing

**Status**: Proposed v1 DTO contract, not implemented. No persistent model or migration.

See [CLI contract](contracts/cli.md) for invocation, exits and schema discovery.
All listed wire fields are required unless explicitly marked optional. Nullable
means the key is present with null, not omitted. Public strings are data, never
instructions; do not interpolate them into commands or terminal control sequences.

## Input And Configuration

`ListOrdersInput` contains side (buy/sell or null), currency (normalized string or
null), and status (required core status, default pending). Side/status accept
case-insensitive input, without whitespace trimming. Currency is uppercased
without trimming, validation or conversion of an empty string to null. No all
status value exists. Shared parsing supplies Clap and operation validation.

`PublicReadConfig` contains parsed Mostro public key and deduplicated validated
relay URLs, CLI values over `MOSTRO_PUBKEY`/`RELAYS`. It has no signer from disk,
pool, identity, admin credentials or trade index. Relay authentication uses fresh
ephemeral in-memory keys. Configuration URLs never appear in public outcomes.

## Envelope

| Field | Wire type | Meaning |
| --- | --- | --- |
| schema_version | string, exactly `1` | Contract version independent of executable version |
| command | string or null | Canonical dotted name, or null when parsing cannot resolve it; `schema` is the discovery name |
| status | string | `ok` or `error` for the pilot; generic v1 reserves `uncertain` for future writes |
| data | typed object or null | ListData or DiscoveryData on success; null on every error |
| error | Error object or null | Null on success, required object on error |
| warnings | Warning array | Empty array when no warning; never null |
| meta | ExecutionMeta object | Explicit execution stages and optional snapshot evidence |

Construct successes and errors through typed variants/constructors so error/data
and status combinations cannot conflict. A command-specific generated schema must
constrain command, success data and status; do not publish an unconstrained object
as the list result. Unknown fields may be ignored by consumers. Within v1 only
optional additive fields are compatible; changing types, semantics, required
fields, status/error enums or enum meanings requires a new contract version.

`Error` has required `code` and `message` strings. Codes come from the contract;
messages are safe allow-listed prose, never raw error chains, argv, relay URLs,
server messages or secrets. `Warning` has the same two fields, with codes
`invalid_event_timestamp` and `cleanup_incomplete`. Repeated invalid timestamps
produce one aggregate warning. Warning text is not an API for client matching.

## ListData And PublicOrder

`ListData` has `node_pubkey` (64 lowercase hex characters), `filters` (the
normalized input above), and `orders` (array of PublicOrder, empty on empty success).
Both renderers consume these fields, including human headings and filter summary.

| PublicOrder field | Wire type | Unit/meaning |
| --- | --- | --- |
| id | string | Canonical lowercase hyphenated UUID |
| side | string | `buy` or `sell`, the advertised order side |
| status | string | One canonical status from the set below |
| sats_amount | decimal string | Signed i64 satoshis; zero retains the existing market-priced meaning |
| fiat_currency | string | Event currency tag; may be empty, no received-data normalization |
| fiat_amount | decimal string | Signed i64 fixed fiat amount in the currency's advertised units, not cents; retained even for range rows |
| fiat_amount_min | decimal string or null | Signed i64 lower range endpoint, same fiat units |
| fiat_amount_max | decimal string or null | Signed i64 upper range endpoint, same fiat units |
| payment_method | string | Public untrusted text, possibly empty; preserve joined tag values |
| premium_percent | decimal string | Signed i64 premium percentage, not a fractional multiplier |
| created_at | string or null | Selected event time in UTC RFC3339 with `Z`; null if not representable |
| created_at_unix | decimal string | Exact decoded signed i64 event timestamp in seconds, not order creation time; event times above i64::MAX saturate to 9223372036854775807 |

Decimal strings are canonical base 10: `0`, positive digits without leading zeros,
or minus followed by nonzero-leading digits. No plus, exponent, fraction or `-0`.
Their semantic range is -9223372036854775808 through 9223372036854775807. Use string
operations or arbitrary-precision integers, not JavaScript Number for decoding.

The status set is: `active`, `canceled`, `canceled-by-admin`, `settled-by-admin`,
`completed-by-admin`, `dispute`, `expired`, `fiat-sent`, `settled-hold-invoice`,
`pending`, `success`, `waiting-buyer-invoice`, `waiting-payment`,
`waiting-taker-bond`, `waiting-maker-bond`, `cooperatively-canceled`, `in-progress`.
Derive/verify this set against pinned core parsing rather than maintaining an
unvalidated second enum.

ID/side are required by the decoder; status is required by the command's filter.
An absent status does not become active in JSON. Preserve the generic decoder's
optional-status behavior for non-command callers. Invalid nonempty status tags
retain the existing pending fallback. Incomplete/invalid ranges remain as decoded;
do not enforce positive amounts, ordered ranges or a valid fiat ticker here.

Reduce latest state by UUID before filtering. Same-UUID timestamp ties retain the
lowest event ID. Order rows sort descending by decoded event timestamp;
cross-order ties have unspecified order. Filtering may return zero rows after a
nonempty fetch. Do not promise at most 50 rows across several relays.

Human rendering uses the same values: zero sats displays market, ranges use
min-max/min-?/ ?-max according to nullable endpoints, and event dates keep the
existing UTC minute display. Invalid dates display Invalid date. The RFC3339
field is null when conversion cannot fit a four-digit UTC calendar year; preserve
the decoded integer and warning rather than failing the whole valid read.

## ExecutionMeta And Snapshot

ExecutionMeta has required `publication`, `daemon_acceptance`, `persistence`, and
`snapshot` fields. For known read-only commands the first three are
`not_applicable`. For a legacy command rejected before execution, or unresolved
invocation, they are `not_attempted`, `unknown`, `not_attempted`. Never claim a
write happened or was accepted from read completion.

Generic envelope v1 stage sets remain the roadmap's: publication = not_attempted,
unknown, published, not_applicable; daemon_acceptance = unknown, accepted,
rejected, not_applicable; persistence = not_attempted, applied, failed,
not_applicable. Pilot schemas restrict these to applicable combinations.

Snapshot is null for discovery, invalid configuration and failures before the
public query starts. Once started it has the following fields, on success or error:

| Field | Type/value | Meaning |
| --- | --- | --- |
| since | UTC RFC3339 string | Clock at query construction minus seven days |
| until | null | No upper-time filter is sent |
| observed_at | UTC RFC3339 string | End of collection, not a completeness guarantee |
| per_relay_limit | integer `50` | Query limit per relay, not global result size |
| max_unique_events | integer `10000` | Collector capacity; exceeding it fails |
| configured_relays | positive integer | Deduplicated relay count |
| completed_relays | nonnegative integer | Relays whose matching EOSE was seen |
| received_events | integer 0..10000 | Unique matching events retained before order decoding/filtering |
| completion_policy | string `all_relays_eose` | Required read-completion evidence |
| complete_history | boolean `false` | Bounded query is never an exhaustive book guarantee |

Counts are bounded small integers, not full-width monetary values. On success,
completed_relays equals configured_relays. On failure data is null even if events
were collected. Reject excess events instead of returning a truncated success.
EOSE does not prove the relay honored the limit or has all historical events.

## DiscoveryData

DiscoveryData has `schema_version` (`1`), `executable_version` (actual build's
package version), `schema_dialect` (`https://json-schema.org/draft/2020-12/schema`),
and `commands` (descriptors sorted by canonical name).

A descriptor has required `name`, `cli_path` (array of invocation tokens),
`output_modes` (human/json for migrated commands, human for legacy), `input_schema`
and `output_schema` (generated JSON Schema objects, both null for legacy coverage
entries), and `required_configuration` (list of environment variable names for
migrated commands, null when legacy requirements are not modeled). Listing names
MOSTRO_PUBKEY and RELAYS, satisfiable by their CLI flags; schema has an empty list.

Legacy entries expose actual existing CLI paths plus their roadmap canonical
names, not an assertion that future noun/verb paths work. Generate current
machine input metadata from Clap/shared validation and outputs from owned DTOs.
Schema documents themselves are object values, avoiding schema-of-schema recursion.
Never read environment values or live defaults while producing discovery.

## Lifecycle And Storage

Listing progresses through validated, connecting, collecting, completed/error,
cleanup, render, and write. Each relay progresses from awaiting EOSE to completed
or failed. Freeze it at first EOSE. Notifications for another subscription or a
completed relay cannot alter the snapshot. Cancellation/disconnect/lag are not
successful completion. Cleanup is bounded; original errors are retained.

No trade state transition, identity lock, DB transaction or migration belongs to
this feature. Future write outcomes/reconciliation remain separate roadmap work.
