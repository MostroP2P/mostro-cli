## Verifying the Release
In order to verify the release, you'll need to have gpg or gpg2 installed on your system. Once you've obtained a copy (and hopefully verified that as well), you'll first need to import the keys that have signed this release if you haven't done so already:
```bash
curl https://raw.githubusercontent.com/MostroP2P/mostro/main/keys/negrunch.asc | gpg --import
curl https://raw.githubusercontent.com/MostroP2P/mostro/main/keys/arkanoider.asc | gpg --import
```
Once you have the required PGP keys, you can verify the release (assuming manifest.txt.sig.negrunch, manifest.txt.sig.arkanoider and manifest.txt are in the current directory) with:
```bash
gpg --verify manifest.txt.sig.negrunch manifest.txt
gpg --verify manifest.txt.sig.arkanoider manifest.txt

gpg: Signature made fri 10 oct 2025 11:28:03 -03
gpg:                using RSA key 1E41631D137BA2ADE55344F73852B843679AD6F0
gpg: Good signature from "Francisco Calderón <fjcalderon@gmail.com>" [ultimate]

gpg: Signature made fri 10 oct 2025 11:28:03 -03
gpg:                using RSA key 2E986CA1C5E7EA1635CD059C4989CC7415A43AEC
gpg: Good signature from "Arkanoider <github.913zc@simplelogin.com>" [ultimate]

```
That will verify the signature of the manifest file, which ensures integrity and authenticity of the archive you've downloaded locally containing the binaries. Next, depending on your operating system, you should then re-compute the sha256 hash of the archive with `shasum -a 256 <filename>`, compare it with the corresponding one in the manifest file, and ensure they match exactly.


## What's Changed in 0.15.0

### 🚀 Features


* bump mostro-core to 0.11.0 and handle anti-abuse bond flow by [@grunch](https://github.com/grunch)
* feat: by [@arkanoider](https://github.com/arkanoider)
* feat(getdmuser): show shared-key DMs (identity + admin) in getdmuser  - Derive shared key from (trade_keys, identity_keys) and fetch/unwrap   gift wraps so DMs sent to the user's identity appear in getdmuser. - Also derive (trade_keys, mostro_pubkey) and fetch so admin replies   from the send_admin_dm_attach flow are shown. - Reuse derive_shared_key_bytes from util/messaging (same ECDH as   send_admin_dm_attach). No longer use get_all_trade_and_counterparty_keys;   counterparty_pubkey is not used in this setup. by [@arkanoider](https://github.com/arkanoider)
* added a command to send dm with attachment to admin to help disputes solving - added some docs for AI generated code by [@arkanoider](https://github.com/arkanoider)

### 🐛 Bug Fixes


* error out on malformed PayBondInvoice payload by [@grunch](https://github.com/grunch)
* fix fmt by [@grunch](https://github.com/grunch)
* scope orders_info request to identity key by [@grunch](https://github.com/grunch)
* sign restore and last-trade-index requests with identity keys by [@grunch](https://github.com/grunch)
* rabbit rants by [@arkanoider](https://github.com/arkanoider)
* Align DM docs/messages and validate POW env parsing by [@arkanoider](https://github.com/arkanoider)
* Blossom upload auth and response handling by [@arkanoider](https://github.com/arkanoider)
* prevent UUID truncation in listorders table by [@codaMW](https://github.com/codaMW)
* meaningful error message and unit tests added by [@arkanoider](https://github.com/arkanoider)
* completed all the paths with new pow management by [@arkanoider](https://github.com/arkanoider)
* fix for giftwrap with pow creation by [@arkanoider](https://github.com/arkanoider)
* correct keys used for send_dm command by [@arkanoider](https://github.com/arkanoider)

### 💼 Other


* feat: bump mostro-core to 0.11.0 and handle anti-abuse bond flow by [@grunch](https://github.com/grunch) in [#166](https://github.com/MostroP2P/mostro-cli/pull/166)
* refactor: migrate gift-wrap to mostro-core 0.10 dual identity/trade keys by [@grunch](https://github.com/grunch) in [#165](https://github.com/MostroP2P/mostro-cli/pull/165)
* refactor: migrate gift-wrap to mostro-core 0.9.1 nip59 module by [@grunch](https://github.com/grunch) in [#164](https://github.com/MostroP2P/mostro-cli/pull/164)
* Shared key chat and attachment feature by [@grunch](https://github.com/grunch) in [#157](https://github.com/MostroP2P/mostro-cli/pull/157)
* Remove unused fiat module by [@grunch](https://github.com/grunch) in [#162](https://github.com/MostroP2P/mostro-cli/pull/162)
* Remove unused fiat module by [@grunch](https://github.com/grunch)
* fix(orders): prevent UUID truncation in listorders table by [@grunch](https://github.com/grunch) in [#159](https://github.com/MostroP2P/mostro-cli/pull/159)
* fix for giftwrap with pow creation by [@grunch](https://github.com/grunch) in [#161](https://github.com/MostroP2P/mostro-cli/pull/161)
* docs: add branch protection rules documentation by [@grunch](https://github.com/grunch) in [#160](https://github.com/MostroP2P/mostro-cli/pull/160)
* Improve MOSTRO_PUBKEY resolution and remove startup panic by [@grunch](https://github.com/grunch) in [#152](https://github.com/MostroP2P/mostro-cli/pull/152)
* resolve MOSTRO_PUBKEY from CLI flag or env without panic by [@codaMW](https://github.com/codaMW)

### 🚜 Refactor


* migrate gift-wrap to mostro-core 0.10 dual identity/trade keys by [@grunch](https://github.com/grunch)
* migrate gift-wrap to mostro-core 0.9.1 nip59 module by [@grunch](https://github.com/grunch)

### 📚 Documentation


* added specifications for direct message and direct message with attachment by [@arkanoider](https://github.com/arkanoider)
* add branch protection rules

### ⚙️ Miscellaneous Tasks


* typos by [@arkanoider](https://github.com/arkanoider)
* typo on folder by [@arkanoider](https://github.com/arkanoider)

## Contributors
* [@grunch](https://github.com/grunch) made their contribution in [#166](https://github.com/MostroP2P/mostro-cli/pull/166)
* [@arkanoider](https://github.com/arkanoider) made their contribution
* [@](https://github.com/) made their contribution
* [@codaMW](https://github.com/codaMW) made their contribution

**Full Changelog**: https://github.com/MostroP2P/mostro-cli/compare/v0.14.5...0.15.0

<!-- generated by git-cliff -->
