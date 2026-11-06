# Implementation Plan: Reply Deadline

**Spec**: [spec.md](spec.md) | **Date**: 2026-11-05 | **Status**: Implemented with Linux evidence against the scripted node.

- `Cli.reply_timeout` (`cli/args.rs`): Clap global, `value_parser!(u64).range(5..=300)`, default 15 (`DEFAULT_REPLY_TIMEOUT`); a range error
  goes through the existing argument-error path, so JSON intent and the command name are kept.
- `cli/write.rs::exchange_limits(cli)`: `ExchangeLimits { reply: cli.reply_timeout seconds, ..default }`, used by the eight dispatch sites that
  built `ExchangeLimits::default()` (`order_new`, `order_take`, `invoice`, `step`, `held`, `migrated` for `order info`).
- Discovery: `DiscoveryData.global_options` from `cli/input_schema.rs::global_options`: every global, non-hidden option of the root, its
  property document, `default`, and `applies_to` from the registry (`--output`: every machine command; `--reply-timeout`: request and write
  commands). Global options are no longer command input (`is_input` skips them).
