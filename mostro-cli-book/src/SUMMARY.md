# Summary

[Introduction](introduction.md)

# Basics

- [Getting started](getting_started.md)
- [Invoking the CLI](invocation.md)
- [The outcome envelope](envelope.md)
- [Status and execution stages](stages.md)
- [Common types](types.md)
- [Error codes](errors.md)
- [Warning codes](warnings.md)

# Commands

- [Command index](commands/README.md)
- [schema](commands/schema.md)
- [order list](commands/order_list.md)
- [order info](commands/order_info.md)
- [order new](commands/order_new.md)
- [order take-sell](commands/order_take_sell.md)
- [order take-buy](commands/order_take_buy.md)
- [order add-invoice](commands/order_add_invoice.md)
- [order fiat-sent](commands/order_fiat_sent.md)
- [order release](commands/order_release.md)
- [order cancel](commands/order_cancel.md)
- [order sync](commands/order_sync.md)
- [bond add-invoice](commands/bond_add_invoice.md)
- [message list](commands/message_list.md)
- [Human-only commands](commands/legacy.md)

# Trade flows

- [Selling as a maker](flows/sell_maker.md)
- [Buying as a taker](flows/buy_taker.md)
- [Buying as a maker](flows/buy_maker.md)
- [Selling as a taker](flows/sell_taker.md)
- [Range orders](flows/range_orders.md)
- [Cancelling](flows/cancel.md)
- [Bonds](flows/bonds.md)
- [Recovering from uncertain outcomes](flows/uncertain.md)

# Building on it

- [Writing a wrapper](wrapper/README.md)
  - [Python](wrapper/python.md)
  - [TypeScript](wrapper/typescript.md)
  - [Rust](wrapper/rust.md)
  - [Shell and jq](wrapper/shell.md)
- [Guide for AI agents](wrapper/ai_agents.md)
- [Decision table](wrapper/decision_table.md)

# Reference

- [Example catalogue](reference/examples.md)
- [Schema files](reference/schemas.md)
- [Order statuses](reference/statuses.md)
- [Node actions in messages](reference/actions.md)
- [Migration from old spellings](reference/migration.md)
