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


## What's Changed in 0.16.2

### 🚀 Features


* reach the solver over the dispute chat
* drop protocol v1 gift-wrap transport by [@arkanoider](https://github.com/arkanoider)
* drop gift-wrap dual-write and dual-read by [@arkanoider](https://github.com/arkanoider)
* make MOSTRO_CHAT_NO_LEGACY actually skip the legacy copy
* migrate peer chat to the gift-wrap-free envelope
* updated rust-toolchain.toml with rust 1.97.0 by [@arkanoider](https://github.com/arkanoider)
* admcancelpending — operator cancel of a pending order over the admin gRPC by [@grunch](https://github.com/grunch)

### 🐛 Bug Fixes


* pin rustls to ring so TLS handshakes do not panic by [@arkanoider](https://github.com/arkanoider)
* parse solver pubkeys and use the chat label by [@arkanoider](https://github.com/arkanoider)
* do not lose what the client learned, and fail closed on migration
* parse trade pubkeys and cover persist by [@arkanoider](https://github.com/arkanoider)
* record the counterparty pubkey after the order row exists
* only trust mostrod when learning the counterparty pubkey
* persist the counterparty trade pubkey from the Order payload
* trim once, English names, and dual-read dedup by [@arkanoider](https://github.com/arkanoider)
* one broken envelope must not hide the other
* do not fail a message that was already published
* use sort_by_key so clippy is clean on rust 1.97 by [@arkanoider](https://github.com/arkanoider)
* clearer MOSTRO_PUBKEY and RELAYS setup errors by [@arkanoider](https://github.com/arkanoider)
* use fiat emoji and drop dead sats label by [@arkanoider](https://github.com/arkanoider)
* show Fiat Amount label for both takebuy and takesell by [@codaMW](https://github.com/codaMW)
* correct amount label and help text for takebuy/takesell by [@codaMW](https://github.com/codaMW)
* show correct label for fiat amount in takebuy by [@codaMW](https://github.com/codaMW)
* admcancelpending refuses daemons that do not enforce pretrade_only by [@grunch](https://github.com/grunch)
* admcancelpending sets pretrade_only so it can never resolve a dispute; doc fixes by [@grunch](https://github.com/grunch)

### 💼 Other


* chore(lib): expose the two persist helpers for library users by [@arkanoider](https://github.com/arkanoider) in [#198](https://github.com/MostroP2P/mostro-cli/pull/198)
* chore(deps): migrate to nostr-sdk 0.45 and latest crate APIs by [@grunch](https://github.com/grunch) in [#197](https://github.com/MostroP2P/mostro-cli/pull/197)
* Update CI workflow to use checkout v6 by [@arkanoider](https://github.com/arkanoider)
* Update CI workflow to build with Rust 1.97.0 by [@arkanoider](https://github.com/arkanoider)
* feat(dispute): reach the solver over the dispute chat by [@arkanoider](https://github.com/arkanoider) in [#196](https://github.com/MostroP2P/mostro-cli/pull/196)
* fix(chat): persist the counterparty trade pubkey from the Order payload by [@arkanoider](https://github.com/arkanoider) in [#195](https://github.com/MostroP2P/mostro-cli/pull/195)
* feat(chat): migrate peer chat to the gift-wrap-free envelope by [@arkanoider](https://github.com/arkanoider) in [#194](https://github.com/MostroP2P/mostro-cli/pull/194)
* chore: relicense project under GPLv3 by [@arkanoider](https://github.com/arkanoider) in [#192](https://github.com/MostroP2P/mostro-cli/pull/192)
* fix: clearer MOSTRO_PUBKEY setup error for first-time users by [@arkanoider](https://github.com/arkanoider) in [#189](https://github.com/MostroP2P/mostro-cli/pull/189)
* fix(take_order): correct amount label and help text for takebuy/takesell by [@arkanoider](https://github.com/arkanoider) in [#158](https://github.com/MostroP2P/mostro-cli/pull/158)
* Merge commit 'acce7197bf66ae6048b86e3092ede6b529851458' into pr/codaMW/158 by [@arkanoider](https://github.com/arkanoider)
* feat: admcancelpending — operator cancel of a pending order over the admin gRPC by [@grunch](https://github.com/grunch) in [#191](https://github.com/MostroP2P/mostro-cli/pull/191)

### 📚 Documentation


* describe kind-14 transport, drop gift wrap by [@arkanoider](https://github.com/arkanoider)
* admcancelpending flag is --orderid by [@grunch](https://github.com/grunch)

### ⚡ Performance


* apply the requested cutoff to the kind 14 query

### ⚙️ Miscellaneous Tasks


* cargo fmt fix by [@arkanoider](https://github.com/arkanoider)
* expose the two persist helpers for library users
* keep the historical MIT notice beside GPLv3 by [@arkanoider](https://github.com/arkanoider)
* relicense project under GPLv3 by [@grunch](https://github.com/grunch)
* label pending_bond_payouts as in-flight bond payouts (mostro#943) by [@grunch](https://github.com/grunch)

## Contributors
* [@arkanoider](https://github.com/arkanoider) made their contribution
* [@](https://github.com/) made their contribution
* [@grunch](https://github.com/grunch) made their contribution in [#197](https://github.com/MostroP2P/mostro-cli/pull/197)
* [@codaMW](https://github.com/codaMW) made their contribution

**Full Changelog**: https://github.com/MostroP2P/mostro-cli/compare/v0.16.1...0.16.2

<!-- generated by git-cliff -->
