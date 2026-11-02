# Status and execution stages

## status

| status | Exit | Meaning | What to do |
| --- | --- | --- | --- |
| `ok` | 0 | The command did what it says. `data` holds the result. | Use `data`. Look at `warnings`. |
| `error` | 1 or 2 | It failed, and the CLI **knows** the node did not act on it (nothing was sent, the node refused, or it was a read). | Branch on `error.code`. Safe to fix and retry. |
| `uncertain` | 1 | A request was (or may have been) published and the CLI cannot prove what happened, **or** the node accepted it and the local write failed. | **Do not retry blindly.** Check the order first. See [Recovering](flows/uncertain.md). |

`uncertain` only exists for commands that publish a write (`order new`, the takes, the
invoices, the trade steps) and for `order sync`'s partial local save. Pure reads
(`order list`, `order info`, `message list`, `schema`) are `ok` or `error` only: they are
safe to repeat.

## The three stages

`meta` describes how far the command got, in three independent dimensions.

### publication — did a request leave?

| Value | Meaning |
| --- | --- |
| `not_applicable` | This command publishes nothing (`order list`, `message list`, `order sync`, `schema`). |
| `not_attempted` | Nothing was published: the command stopped before (bad arguments, no identity, refused PoW...) or every relay explicitly rejected the event. |
| `published` | At least one relay confirmed accepting the signed request. |
| `unknown` | No relay confirmed, but at least one failed ambiguously (timeout, dropped connection after the send): the event may be out there. |

### daemon_acceptance — what did the Mostro node say?

| Value | Meaning |
| --- | --- |
| `not_applicable` | No node request is involved. |
| `unknown` | No usable answer (not sent, timed out, malformed). |
| `accepted` | The node answered with the expected action. |
| `rejected` | The node answered `cant-do` (`error.code` is `node_refused`). |

### persistence — did the local write happen?

| Value | Meaning |
| --- | --- |
| `not_applicable` | This command never writes locally (`order list`, `order info`, `message list`, `schema`). |
| `not_attempted` | No write was called for, or the command stopped before writing. |
| `applied` | The write the outcome calls for was done (an order row, a status update, a reserved trade index). |
| `failed` | The write was attempted and failed. |

What "the write" is depends on the command:

| Command | Local write |
| --- | --- |
| `order new`, `order take-*` | Reserve/advance the trade index; save the order row with its trade key |
| `order add-invoice` | Update the order's local status |
| `order fiat-sent`, `order release`, `order cancel` | Update the order's local status; for a range maker, record the next trade index |
| `bond add-invoice` | Nothing (`not_attempted` on success) |
| `order sync` | Insert rows for orders the store did not know |

## Allowed combinations

The generated schemas enforce exactly these combinations. The tables read
`publication / daemon_acceptance / persistence`.

### Reads: `order list`, `message list`, `schema`

| status | Stages |
| --- | --- |
| `ok` | `not_applicable / not_applicable / not_applicable` |
| `error` | `not_applicable / not_applicable / not_applicable` |

### Request without local write: `order info`

| status | Stages | Typical codes |
| --- | --- | --- |
| `ok` | `published / accepted / not_applicable` | |
| `error` | `not_attempted / unknown / not_applicable` | local failures, `pow_required`, `request_rejected` |
| `error` | `unknown / unknown / not_applicable` | `request_rejected` (ambiguous relay) |
| `error` | `published / unknown / not_applicable` | `reply_timeout`, `reply_malformed`, `relay_read_failed`, late `pow_required` |
| `error` | `published / rejected / not_applicable` | `node_refused` |

### Creating: `order new`, `order take-sell`, `order take-buy`

| status | Stages | Typical codes |
| --- | --- | --- |
| `ok` | `published / accepted / applied` | |
| `error` | `not_attempted / unknown / not_attempted` | local failures, proven `request_rejected` |
| `error` | `published / rejected / not_attempted` | `node_refused` |
| `uncertain` | `published / accepted / failed` | `order_not_saved` (data present) |
| `uncertain` | `published / unknown / applied` or `failed` | `reply_timeout`, `reply_malformed`, `relay_read_failed`, `pow_required` |
| `uncertain` | `unknown / unknown / applied` or `failed` | `request_rejected` |

Here `persistence` on an uncertain outcome is about the **trade index**: it is spent
whenever the request may have been published, so the next attempt uses a fresh key.

### Updating: `order add-invoice`, `order fiat-sent`, `order release`, `order cancel`

| status | Stages | Typical codes |
| --- | --- | --- |
| `ok` | `published / accepted / applied` | |
| `ok` | `published / accepted / not_attempted` | nothing to record (`cancel_requested`, `invoice_updated`) |
| `ok` | `published / accepted / failed` | with warning `local_status_stale` or `trade_index_not_recorded` |
| `error` | `not_attempted / unknown / not_attempted` | local failures, `order_unknown` |
| `error` | `published / rejected / not_attempted` | `node_refused` |
| `uncertain` | `published / unknown / not_attempted` | `reply_timeout`, `reply_malformed`, `relay_read_failed`, `pow_required` |
| `uncertain` | `unknown / unknown / not_attempted` | `request_rejected` |

For the trade steps (`fiat-sent`, `release`, `cancel`) an uncertain outcome may also show
`applied` or `failed`: the maker of a range order reserved a trade index for the child
order, and persistence tells whether it was recorded.

### `bond add-invoice`

| status | Stages |
| --- | --- |
| `ok` | `published / accepted / not_attempted` |
| `error` | `not_attempted / unknown / not_attempted` or `published / rejected / not_attempted` |
| `uncertain` | `published / unknown / not_attempted` or `unknown / unknown / not_attempted` |

### `order sync`

| status | Stages | Meaning |
| --- | --- | --- |
| `ok` | `not_applicable / not_applicable / applied` | At least one row saved |
| `ok` | `not_applicable / not_applicable / not_attempted` | Nothing to save |
| `error` | `not_applicable / not_applicable / not_applicable` or `not_attempted` | Failed before or after reading, before saving |
| `uncertain` | `not_applicable / not_applicable / failed` | Some rows saved, then one failed (`order_not_saved`) |

### Unresolved or rejected before running

| Situation | Stages |
| --- | --- |
| `command: null` (unknown command, removed spelling) | `not_attempted / unknown / not_attempted` |
| Legacy command with `--output json` (`output_not_supported`) | `not_attempted / unknown / not_attempted` |

## Reading stages in a wrapper

Most of the time `status` + `error.code` is enough. Stages answer two finer questions:

- **"Could the node have acted?"** — `publication` is `published` or `unknown`.
- **"Is my local store in step?"** — `persistence` is `failed` ⇒ the node moved on but the
  local row did not; follow the warning or error advice (often: `order info`, `order sync`).
