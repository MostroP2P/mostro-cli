# Specification Quality Checklist: Order Show

**Purpose**: Review specification completeness before technical planning.

**Created**: 2026-10-28

**Feature**: [Order-show specification](../spec.md)

## Content Quality

- [x] Describes user-visible behavior without prescribing internal modules or dependencies.
- [x] Focuses on human and automation use cases.
- [x] Explains why each user story matters.
- [x] Completes all required specification sections.

## Requirement Completeness

- [x] No unresolved template or clarification markers remain. (Missing identity: fail, never create. Ids per call: no client cap, the daemon enforces its limits. Both recorded in the spec's clarifications, 2026-10-28.)
- [x] Functional requirements have observable pass/fail outcomes.
- [x] Success criteria are measurable through command behavior.
- [x] Criteria avoid internal implementation choices.
- [x] Acceptance scenarios cover populated, empty, refused, timed-out and failed results.
- [x] Edge cases cover correlation, identity, output selection, untrusted text and secrets.
- [x] Scope excludes `order new`, `order take`, other write commands and the MCP migration.
- [x] Dependencies on the delivered pilot and the roadmap are explicit.

## Feature Readiness

- [x] Requirements map to the three stories and their acceptance scenarios.
- [x] Human, automation and offline discovery flows are represented.
- [x] Outcomes cover parseable results, data parity, failures and the single published request.
- [x] The specification leaves data shapes, reply decoding and module choices to the technical plan.

## Review Notes

This is the built-in specification-quality review, not implementation approval or a
runtime test report. The spec is a draft: it has no plan, tasks or evidence, and its
acceptance criteria are unverified. Behavior against a real daemon is not known and
must be stated by the plan.
