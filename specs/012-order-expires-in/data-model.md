# Data Model: Order Expiry In Seconds

No output change: `order.new` data is unchanged (the order the node returns already carries its `expires_at`).

| Input | Request `expires_at` |
| --- | --- |
| neither option, or `--expiration-days 0` | none: the node's default |
| `--expiration-days D` (1..365) | `now + D * 86400` |
| `--expires-in S` (1..31536000) | `now + S` |
| both, or `S` outside its range | `invalid_arguments`, nothing sent |
