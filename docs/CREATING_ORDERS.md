# Creating Orders in Mostro CLI

This document explains how to create new buy and sell orders using Mostro CLI.

## Overview

Creating an order involves:

1. Specifying order parameters (type, amount, currency, etc.)
2. Building a Mostro message
3. Sending it to the Mostro coordinator via Nostr
4. Receiving confirmation
5. Waiting for someone to take the order

## Order Types

### Sell Order (Maker sells Bitcoin)

User wants to **sell Bitcoin** for fiat currency.

```bash
mostro-cli neworder -k sell -c USD -f 100 -a 50000 -m "PayPal"
```

### Buy Order (Maker buys Bitcoin)

User wants to **buy Bitcoin** with fiat currency.

```bash
mostro-cli neworder -k buy -c EUR -f 200 -m "Bank Transfer" -i lnbc...
```

## Order Parameters

### Required Parameters

| Parameter | Flag | Description | Example |
| ----------- | ------ | ------------- | --------- |
| Kind | `-k`, `--kind` | "buy" or "sell" | `sell` |
| Fiat Code | `-c`, `--fiat-code` | Currency code | `USD`, `EUR`, `ARS` |
| Fiat Amount | `-f`, `--fiat-amount` | Amount in fiat | `100` or `100-500` (range) |
| Payment Method | `-m`, `--payment-method` | How to pay | `"PayPal"`, `"Bank Transfer"` |

### Optional Parameters

| Parameter | Flag | Description | Default |
| ----------- | ------ | ------------- | --------- |
| Amount | `-a`, `--amount` | Bitcoin in sats | 0 (market price) |
| Premium | `-p`, `--premium` | Price premium % | 0 |
| Invoice | `-i`, `--invoice` | Lightning invoice (buy orders) | None |
| Expiration Days | `-e`, `--expiration-days` | Days until expired | 0 |

## Order Examples

### 1. Simple Sell Order (Market Price)

```bash
mostro-cli neworder -k sell -c USD -f 100 -m "PayPal"
```

### 2. Range Order (Flexible Amount)

```bash
mostro-cli neworder -k sell -c USD -f 100-500 -m "PayPal,Venmo"
```

### 3. Buy Order with Lightning Invoice

```bash
mostro-cli neworder -k buy -c USD -f 50 -i lnbc500u1p3.... -m "Cash App"
```

For technical details on the code flow and message structures, see [ORDER_FLOW_TECHNICAL.md](./ORDER_FLOW_TECHNICAL.md).
