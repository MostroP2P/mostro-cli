# Data Model: Mine The Proof Of Work The Node Advertises

## `error` on `pow_required`

| Key | Type | Meaning |
| --- | --- | --- |
| `code` | `"pow_required"` | unchanged |
| `message` | string | unchanged fixed text |
| `required` | integer 0-255 | the proof of work, in bits, the node advertises for the request's lane; on the timeout path, the highest advertised difficulty above the work sent |
| `configured` | integer 0-255 | the `--pow` / `POW` floor (0 when unset) |

Both are present on every `pow_required` and absent on every other code. Example (`publication: not_attempted`):

```json
{"code": "pow_required", "message": "The node requires more proof of work than is configured.", "required": 21, "configured": 0}
```

## Lanes

| Lane | Commands | `required` |
| --- | --- | --- |
| known | `order add-invoice`, `bond add-invoice`, `order fiat-sent`, `order release`, `order cancel`, `dispute open` | `pow` |
| first contact | `order new`, `order take-sell`, `order take-buy`, `order info`, `order rate` | `max(pow, pow_first_contact)` |
