# Taking Orders in Mostro CLI

This document explains how to take existing orders from the Mostro order book.

## Overview

Taking an order means accepting someone else's offer to buy or sell Bitcoin. When you take an order:
1. You become the **taker** (they are the **maker**).
2. The order moves from `Pending` to `Active`.
3. You and the maker are matched for a trade.
4. The trade process begins based on the order type.

## Order Types from Taker's Perspective

### Taking a Sell Order (TakeSell)
**Scenario**: Someone wants to sell Bitcoin, you want to buy it.
```bash
mostro-cli takesell -o <order-id> -i <lightning-invoice> -a <sats>
```
- **Your role**: Buyer.
- **You provide**: Lightning invoice to receive Bitcoin.
- **You send**: Fiat payment to the seller.

### Taking a Buy Order (TakeBuy)
**Scenario**: Someone wants to buy Bitcoin, you want to sell it.
```bash
mostro-cli takebuy -o <order-id> -a <sats>
```
- **Your role**: Seller.
- **You provide**: Bitcoin by paying a hold invoice.
- **You receive**: Fiat payment from the buyer.

## Commands

### List Available Orders
Before taking an order, find one that suits your needs:
```bash
mostro-cli listorders --status pending
```

### Take Sell Order (Buy Bitcoin)
```bash
mostro-cli takesell -o eb5740f6-e584-46c5-953a-29bc3eb818f0 -i lnbc500u1...
```

### Take Buy Order (Sell Bitcoin)
```bash
mostro-cli takebuy -o eb5740f6-e584-46c5-953a-29bc3eb818f0
```

## Implementation Highlights
The take logic is handled in `src/cli/take_order.rs`. It follows this pattern:
1. Derive **new** trade keys for the taker.
2. Send a `TakeSell` or `TakeBuy` message to Mostro.
3. Wait for Mostro's confirmation (and hold invoice if selling).
4. Save the order state locally.

For detailed trade flows and status explanations, see [TRADE_LIFECYCLE.md](./TRADE_LIFECYCLE.md).
