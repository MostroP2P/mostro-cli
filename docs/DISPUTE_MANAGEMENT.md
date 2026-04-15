# Dispute Management

This document covers how disputes are handled in Mostro CLI by both users and administrators.

## User Dispute Flow

When a trade goes wrong (e.g., fiat sent but Bitcoin not released), either party can initiate a dispute.

### Initiate a Dispute

```bash
mostro-cli dispute -o <order-id>
```

Mostro changes the order status to `Dispute`. This prevents any further automated transitions and flags the trade for manual intervention.

## Admin/Solver Flow

Admins or designated solvers use special commands to resolve conflicts. These commands require the `ADMIN_NSEC` environment variable to be set.

### 1. List Active Disputes

```bash
mostro-cli listdisputes
```

### 2. Take a Dispute

Before resolving, an admin must "take" the dispute to indicate they are handling it.

```bash
mostro-cli admtakedispute -d <dispute-id>
```

### 3. Settle (Pay Buyer)

If the buyer proved they sent fiat, the admin settles the hold invoice to pay the buyer.

```bash
mostro-cli admsettle -o <order-id>
```

### 4. Cancel (Refund Seller)

If the buyer failed to pay, the admin cancels the order to refund the locked Bitcoin to the seller.

```bash
mostro-cli admcancel -o <order-id>
```

## Security

Admin commands are gated by public key verification on the Mostro coordinator side. The CLI must sign these messages with the private key corresponding to a registered admin pubkey.
