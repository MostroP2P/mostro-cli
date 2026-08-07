# Security Policy

Mostro CLI is a non-custodial client for the Mostro P2P exchange protocol. It handles
seed phrases, derived trade keys and encrypted Nostr messages, so security reports are
taken seriously and handled with priority.

## Supported Versions

Only the latest published release line receives security fixes. Users are expected to
upgrade to the most recent release before reporting an issue.

| Version | Supported          |
| ------- | ------------------ |
| 0.16.x  | Yes                |
| < 0.16  | No                 |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report vulnerabilities privately by email to **security@mostro.network**.

Please include as much of the following as possible:

- A description of the vulnerability and its potential impact.
- The affected version, commit hash or tag.
- Step-by-step instructions to reproduce the issue.
- Any proof-of-concept code, logs or configuration required to trigger it.
- Your assessment of severity and any suggested mitigation.

Reports written in English or Spanish are both welcome.

### What to expect

| Stage                      | Target                       |
| -------------------------- | ---------------------------- |
| Acknowledgement of receipt  | Within 72 hours              |
| Initial assessment          | Within 7 days                |
| Fix or mitigation plan      | Within 30 days for confirmed issues |

If a report requires more time, we will keep you informed of the progress. Critical
issues affecting user funds or key material are prioritized over the stated targets.

## Disclosure Policy

We follow coordinated disclosure:

1. You report the issue privately by email.
2. We confirm the issue and work on a fix.
3. A patched release is published and users are notified.
4. Details are disclosed publicly once a fix is available, or after 90 days from the
   initial report, whichever comes first.

Please do not disclose the issue publicly before a fix is released, unless we have
agreed otherwise.

## Scope

### In scope

- Exposure or leakage of the mnemonic, identity key or derived trade keys.
- Weaknesses in key derivation, storage or database encryption.
- Flaws in the encryption, signing or validation of Nostr messages
  (NIP-06, NIP-44, NIP-59, NIP-98).
- Improper validation of messages received from a Mostro instance or a counterpart
  that leads to loss of funds, impersonation or privacy loss.
- Insecure file permissions or insecure defaults in the CLI data directory.
- Handling of Lightning invoices and LNURL that could redirect or steal funds.
- Dependency vulnerabilities with a demonstrable impact on this client.

### Out of scope

- Vulnerabilities in the Mostro daemon. Report those to the
  [mostro](https://github.com/MostroP2P/mostro) repository.
- Vulnerabilities in third-party Nostr relays, Lightning wallets or LNURL services.
- Issues that require prior physical access to an unlocked machine, or a host already
  compromised by malware.
- Attacks that require the user to voluntarily disclose their mnemonic.
- Denial of service against public relays.
- Reports produced solely by automated scanners without a demonstrated impact.

## Security Considerations for Users

- `~/.mcli/mcli.db` stores your mnemonic. Treat it like a wallet seed file: keep
  permissions restricted (`chmod 600`), do not sync it to clear-text cloud backups and
  do not share it.
- `ADMIN_NSEC` grants administrative capabilities over a Mostro instance. Keep it out of
  shell history and shared environments.
- Verify the `MOSTRO_PUBKEY` you configure. Connecting to an untrusted instance exposes
  you to counterpart and coordinator misbehaviour.
- Always upgrade to the latest release to receive security fixes.

## Recognition

Reporters who follow this policy will be credited in the release notes of the fix,
unless they prefer to remain anonymous.
