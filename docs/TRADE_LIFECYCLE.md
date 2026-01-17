# Trade Lifecycle and Status Flow

This document describes the complete lifecycle of a Mostro trade, from matching to completion.

## Order Status Transitions

```
┌─────────────┐
│   PENDING   │ ← Order created by maker
└──────┬──────┘
       │ Someone takes the order
       ↓
┌─────────────┐
│   ACTIVE    │ ← Maker and taker matched
└──────┬──────┘
       │ Seller pays hold invoice
       ↓
┌─────────────┐
│ WAITING_    │
│ BUYER_      │ ← Waiting for buyer's invoice (if not provided)
│ INVOICE     │
└──────┬──────┘
       │ Buyer adds invoice
       ↓
┌─────────────┐
│  WAITING_   │
│  PAYMENT    │ ← Waiting for fiat payment
└──────┬──────┘
       │ Buyer sends fiat
       ↓
┌─────────────┐
│ FIAT_SENT   │ ← Fiat payment claimed by buyer
└──────┬──────┘
       │ Seller confirms & releases
       ↓
┌─────────────┐
│   SUCCESS   │ ← Trade completed!
└─────────────┘
```

## Detailed Example: Alice Buys from Bob

1. **Take**: Alice takes Bob's sell order using `takesell`.
2. **Lock**: Bob receives a `pay-invoice` message. He pays the hold invoice to lock his Bitcoin.
3. **Pay**: Alice sees the status change to `WaitingPayment`. She sends fiat to Bob via the agreed method (e.g., PayPal).
4. **Confirm**: Alice runs `mostro-cli fiatsent`.
5. **Release**: Bob confirms the fiat arrived and runs `mostro-cli release`.
6. **Done**: Alice receives her Bitcoin automatically.

## Error States

- **Canceled**: One party canceled the order (only possible in early stages).
- **Dispute**: A party disagreed with the trade progress. See [DISPUTE_MANAGEMENT.md](./DISPUTE_MANAGEMENT.md).
- **Expired**: The order wasn't taken or advanced within the required timeframe.

## Common Troubleshooting

### "Invoice already used"
Generate a fresh invoice in your wallet. Mostro requires a unique payment hash for every trade.

### "No response from Mostro"
Check your relay connections or verify if the Mostro coordinator is online. Use `--verbose` to see network logs.
