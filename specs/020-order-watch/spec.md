# Feature Specification: Follow An Order Until The Trade Ends

**Feature**: `020-order-watch`

**Working Branch**: `develop` (feature directory naming does not change Git branches)

**Created**: 2026-11-09

**Status**: In progress, unit 1 of 7: the contract types and the constitution amendment. Nothing is user-visible yet. Progress is
tracked only in [tasks.md](tasks.md)

**Input**: Issue #246. An agent that ran a full trade with mostro-cli 0.17.0 had to poll `message list` and `getdmuser` every 15 seconds
to follow it. Polling sees a step up to one interval late, reconnects to every relay on every poll, and leaves the caller to remember what
it has seen, to run two commands and to decide when the trade is over. Proposed: `order watch --order <uuid>`, which follows the node's
messages to the order's trade key and, once the counterparty is known, the order's peer chat. It replays the window, then goes live. The
issue leans towards option (a) for JSON: one document per call as soon as at least one new record exists, or an empty list at `--timeout`,
with a `cursor` passed back with `--after`; human mode streams until the trade ends. The maintainers chose (a). It reuses #245
(`order chat`, [spec 019](../019-order-chat/spec.md)): the counterparty lookup, the chat transport and `ChatRecord`.

## Clarifications

### Session 2026-11-09 (the design; D7, D12 and D13 are flagged for the maintainers' confirmation)

D2, D5, D7, D8, D9 and D14 depart from the issue's wording; each says why.

- **D1 Grammar**: `order watch --order <uuid> [--since <minutes> | --after <cursor>] [--timeout <seconds>]`, canonical `order.watch`.
  `--since` is 1 to 10080 minutes, default 30. `--after` conflicts with `--since` (the grammar reports `invalid_arguments`). `--timeout` is
  1 to 86400 seconds. A bad value is `invalid_arguments` (exit 2) before any configuration, store or relay access, and is never echoed.
- **D2 Deadline (departure: no dedicated code)**: JSON waits 30 s by default; human mode has no default deadline. The deadline counts
  from the end of the first replay. At the deadline both modes get the same outcome: `ok`, `ended: false`, empty lists and the cursor
  carried forward. JSON emits that document; human mode prints "⏰ Stopped at the watch deadline; the trade has not ended." and exits 0.
  The issue's dedicated exit code is dropped: no extra codes, and Principle IV gives both renderers one outcome.
- **D3 Subscriptions**: each relay gets one REQ per filter, sent with `ReqTarget::single` from one `.subscribe(` site: the node mail
  (`read_inbox`'s filter for the order's trade key, since S, limit 50) and, once the counterparty is known, the chat
  (`chat_filter(K_sign)`, since S, limit 50, with `K_sign` from `derive_chat_keys(trade, peer)`). Separate REQs keep both filters
  working on relays that take one filter per REQ. Notifications are armed before any REQ.
- **D4 Replays**: the opening replay is strict, as for the reads: every relay must send EOSE within 15 s (otherwise `read_timeout`);
  before EOSE a CLOSED, a disconnect, the end of the stream or the notification capacity is `relay_read_failed`. Once live, no relay
  event ends the watch. A replay starts on a relay with any later REQ (the chat REQ, or a subscription sent again) and with every
  reconnect, because the SDK sends its subscriptions again. New records are held while any relay replays; a replay ends at that relay's
  EOSE for the subscription, at a disconnect, or after 15 s. `truncated` is set when a replay returns 50 stored events for a subscription
  and the oldest of them was neither seen in this run nor listed in the cursor, or when a live replay reaches the notification capacity
  before EOSE.
- **D5 Reconnect (departure: no renewal from the newest time)**: the watch's session reconnects on its own with a fixed retry (10 s),
  and the SDK sends again the subscriptions it still holds, with their original filter (since S, limit 50). When a relay reports
  Connected, each watch subscription that relay no longer holds (after a CLOSED, or one that could not be sent while it was down) is sent
  again with a new id and the same filter. Duplicates are dropped (D8). A relay that refuses again while it stays connected is not asked
  again until it reconnects. Why: the stored limit returns the newest 50 since S, so what was missed during an outage arrives unless more
  than 50 arrived (then `truncated`); a since at the newest time would lose lines stamped by a slower clock and would race the SDK's own
  resend.
- **D6 Counterparty (reuses #245)**: at the start, `find_counterparty`: the row's key when it parses and is not ours, else a since-0
  read of the node's mail, which caches the key it finds (a failed cache write is not reported, as in spec 019 D3).
  `counterparty_unknown` starts the watch without the chat; any other failure keeps its code and warnings. While no key is known, each new
  node message goes through `counterparty_in`; the first match sends the chat REQs (a replay since S, D4) and fills
  `data.counterparty_pubkey`. A key learned while watching is not saved; the next call's lookup caches it.
- **D7 Cursor (flagged; departure: not "the ids at that second")**: opaque, `<since>` or `<since>:<id>,...`: decimal Unix seconds with
  no sign and no leading zeros, then 1 to 200 distinct lowercase 64-hex ids, written sorted. It covers accepted records only: node
  events that pass `open_events` and chat events that pass `open_lines`, each routed by author (node to mail, `K_sign` to chat, anything
  else dropped and not counted). A record's id is its event id (mail) or its inner `message_id` (chat); its time is the outer
  `created_at`, which relays filter on. The next cursor's time is the newest observed time, clamped to local now, minus 60 s
  (`CHAT_MAX_CLOCK_SKEW_SECS`), never lower than the old one; while every envelope seen of a chat message falls before that time and
  another envelope of it could still be dated at or after it (mostro-core accepts one dated up to 60 s after the inner `created_at`, and
  the counterparty can re-wrap any line), the time is held at that message's newest envelope, so the next call observes it again and
  skips it (a re-published node event keeps its time); above 200 records at or after the time, it rises to the smallest second that
  leaves at most 200, but never past local now minus 60 s (nor past the old time when that is later), so a raise covers only seconds
  observed in the call that ended more than 60 s before, whatever times the records claim (a counterparty can date chat envelopes up to
  60 s ahead). Its ids are every record observed in the call, new or skipped, at or after that time; if more than 200 still qualify, the
  cursor lists the newest 200 and a later call returns the others again. An empty observation keeps the cursor; a call without `--after`
  starts from `<S>`. `--after` subscribes from the cursor's time and skips the listed ids while still observing them. **Documented
  limits**: a record that reaches the relays after records stamped more than 60 s later, or one inside a raised range, is never returned
  in the JSON loop, and a chat message inside a raised range is returned again if another envelope of it, dated after the range, reaches
  the relays; when more than 200 records fall within the last minute, a later call returns some of them again.
- **D8 Records (departure: two arrays)**: `messages` are `MessageRecord`s as `message list` builds them, `next_step` included, with the
  `trade_index` of the order's key. `chat` holds #245's `ChatRecord`s; chat events are opened one at a time, so `open_lines`'s cap of 200
  never applies. Each array runs oldest first by (outer time, id). Mail is deduplicated by outer id and chat by inner `message_id`, across
  relays, resends and calls (the cursor). At most 10,000 accepted records per run; beyond that the run ends with `read_limit_exceeded`.
- **D9 End (departure: `admin-settled` depends on the side)**: `ended` is true when a returned node message about this order carries
  `purchase-completed`, `hold-invoice-payment-settled`, `canceled`, `cooperative-cancel-accepted` or `admin-canceled`, or `admin-settled`
  when the local row says the user is the seller (`TradeRole::of_order(kind, is_mine)`). The node pays the buyer only after
  `admin-settled`, so a buyer's watch goes on to `purchase-completed`, or to `payment-failed` and `add-invoice`. An unknown side
  (`is_mine` NULL) does not end on `admin-settled`. `hold-invoice-payment-canceled`, `released`, `rate`, `payment-failed` and
  `add-invoice` do not end the watch. Documented, not detected: `rate` follows a successful end and `order rate` does not need it;
  `bond-slashed` and `add-bond-invoice` can follow an end (see `message list --order`); an untaken order that expires sends the maker
  nothing; an end before the window is not seen. For these the docs advise `--timeout` and `order info`.
- **D10 Interrupt**: no signal handler; Ctrl-C ends the process by its default action. Human output is flushed after each update; JSON
  prints no document, the contract's existing exception for a killed process. No exit status is promised. A killed JSON call is safe to
  repeat with the same cursor.
- **D11 Stages and warnings**: registry `JsonSupport::Read`; every stage `not_applicable` and a null snapshot; rules
  `OutcomeRules::read(COMMAND, &ERRORS, false)`, so the warnings are the pilot pair (`invalid_event_timestamp`, `cleanup_incomplete`).
  The lookup's cache write is neither a stage nor a warning, as for `order.chat`. `--pow`, `--transport` and `--reply-timeout` are not
  read. `schema_version` stays `1`: the command is additive.
- **D12 Rollout and the schema file (flagged)**: unit 5 ships human mode (`JsonSupport::Unsupported`, discovery +12); unit 6 turns JSON on
  and regenerates discovery (about +880); unit 7 adds `docs/schemas/order-watch-v1.json` (842 lines measured) and its drift test. The two
  generated files cannot share one unit under 1500 changed lines, so between units 6 and 7 the output schema is checked only inside
  `discovery-v1.json`.
- **D13 Constitution (flagged)**: amendment 1.0.1, a clarification of the Project Boundaries: they exclude streaming machine output, and
  human-mode `order watch` may stream updates until the trade ends, with no overall deadline unless `--timeout` is given and with bounded
  relay steps. Recorded in the constitution's amendment record (unit 1).
- **D14 Input schema (departure)**: the issue asks for input and result schemas under `docs/schemas/`. By the repository's convention the
  input schema lives only in the discovery entry, as spec 019 D9 records; only the output schema gets its own file.

## Requirements

- **FR-001**: `mostro-cli order watch --order <uuid> [--since <minutes> | --after <cursor>] [--timeout <seconds>]`, canonical
  `order.watch`, migrated; needs `MOSTRO_PUBKEY`, `RELAYS`, the identity and the order's row.
- **FR-002**: The watch follows the node's mail to the order's trade key and, once the counterparty is known (D6), the order's chat:
  replay since the window or the cursor, then live (D3, D4, D5).
- **FR-003**: Each accepted record is returned once, within the limits of D7 (D7, D8); nothing unopened or foreign counts.
- **FR-004**: JSON returns one document per call at the first new records after the replays, or empty at the deadline (D2), with the
  cursor; human mode streams until the trade ends (D9) or the deadline (D13).
- **FR-005**: Untrusted text is bounded by its records and escaped in human output; it never reaches errors, verbose output or stderr.
  Nothing is published and no secret is serialized.
- **FR-006**: Output schema `docs/schemas/order-watch-v1.json`, the discovery entry and drift tests (D12, D14); `schema_version` stays `1`.

## Success Criteria

- **SC-001**: The bounds and defaults of D1 and D2 and the cursor rules of D7 hold as unit tests: a cursor round-trips, malformed ones are
  refused, and the next cursor keeps a minute of ids, clamped to now and capped at 200, records stamped ahead never move its time past
  now minus 60 s, and a chat message stays listed while another envelope of it could still reach the time (unit 1).
- **SC-002**: Only the actions of D9 end a trade, `admin-settled` only for the seller (unit 1).
- **SC-003**: With two loopback relays, the window replays first and a new node message then arrives live, once; the chat joins once the
  counterparty is known; a dropped relay reconnects and misses nothing (units 2 and 3).
- **SC-004**: A JSON call with the returned cursor returns only a record published while it waits; at the deadline it is an empty success;
  a human watch ends at `purchase-completed` with exit 0 (units 4 to 6).
