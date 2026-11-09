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


## What's Changed in 0.17.1

### 🚀 Features


* name the config commands in setup hints by [@grunch](https://github.com/grunch)
* add config show by [@grunch](https://github.com/grunch)
* add config set by [@grunch](https://github.com/grunch)
* add the typed core of config set by [@grunch](https://github.com/grunch)
* read the config file in the human-only commands by [@grunch](https://github.com/grunch)
* read the node settings from the config file by [@grunch](https://github.com/grunch)
* ask the seller to release in message records by [@grunch](https://github.com/grunch)
* ask for fiat-sent and rate in message records by [@grunch](https://github.com/grunch)
* type the payment_failed payload in messages by [@grunch](https://github.com/grunch)
* add JSON output to order watch by [@grunch](https://github.com/grunch)
* add order watch by [@grunch](https://github.com/grunch)
* add the order watch operation by [@grunch](https://github.com/grunch)
* keep an order watch across relay reconnects by [@grunch](https://github.com/grunch)
* follow an order's mail and chat live by [@grunch](https://github.com/grunch)
* define the order watch contract by [@grunch](https://github.com/grunch)
* add JSON output to order chat by [@grunch](https://github.com/grunch)
* add order chat by [@grunch](https://github.com/grunch)
* read, send and render the peer chat by [@grunch](https://github.com/grunch)
* find the counterparty of an order by [@grunch](https://github.com/grunch)
* mine the proof of work the node advertises by [@grunch](https://github.com/grunch)

### 🐛 Bug Fixes


* give a blank MOSTRO_PUBKEY the missing message by [@grunch](https://github.com/grunch)
* harden and sync the config file write by [@grunch](https://github.com/grunch)
* keep truncated from the batch that ends the grace by [@grunch](https://github.com/grunch)
* stop an idle watch when its reader closes stdout by [@grunch](https://github.com/grunch)
* point the deadline line to order info by [@grunch](https://github.com/grunch)
* judge a live replay cut by CLOSED for truncated by [@grunch](https://github.com/grunch)
* list node ids ahead of chat ids at the cursor cap by [@grunch](https://github.com/grunch)
* bound the watch replay wait by its budget by [@grunch](https://github.com/grunch)

### 💼 Other


* test: use an escape the order table never emits by [@grunch](https://github.com/grunch) in [#267](https://github.com/MostroP2P/mostro-cli/pull/267)
* feat: ask the seller to release in message records (#243 3/3) by [@grunch](https://github.com/grunch) in [#266](https://github.com/MostroP2P/mostro-cli/pull/266)
* Merge remote-tracking branch 'origin/main' into feat/release-step by [@grunch](https://github.com/grunch)
* feat: ask for fiat-sent and rate in message records (#243 2/3) by [@grunch](https://github.com/grunch) in [#265](https://github.com/MostroP2P/mostro-cli/pull/265)
* Merge remote-tracking branch 'origin/main' into feat/fiat-sent-and-rate-steps by [@grunch](https://github.com/grunch)
* feat: type the payment_failed payload in messages (#243 1/3) by [@grunch](https://github.com/grunch) in [#264](https://github.com/MostroP2P/mostro-cli/pull/264)
* Merge remote-tracking branch 'origin/main' into feat/payment-failed-payload by [@grunch](https://github.com/grunch)
* test: check order watch against its schema file (#246 7/7) by [@grunch](https://github.com/grunch) in [#263](https://github.com/MostroP2P/mostro-cli/pull/263)
* Merge remote-tracking branch 'origin/main' into feat/order-watch-schema by [@grunch](https://github.com/grunch)
* feat: add JSON output to order watch (#246 6/7) by [@grunch](https://github.com/grunch) in [#262](https://github.com/MostroP2P/mostro-cli/pull/262)
* Merge remote-tracking branch 'origin/main' into feat/order-watch-json by [@grunch](https://github.com/grunch)
* feat: add order watch (#246 5/7) by [@grunch](https://github.com/grunch) in [#261](https://github.com/MostroP2P/mostro-cli/pull/261)
* Merge remote-tracking branch 'origin/main' into feat/order-watch-command by [@grunch](https://github.com/grunch)
* feat: add the order watch operation (#246 4/7) by [@grunch](https://github.com/grunch) in [#260](https://github.com/MostroP2P/mostro-cli/pull/260)
* Merge remote-tracking branch 'origin/main' into feat/order-watch-operation by [@grunch](https://github.com/grunch)
* feat: keep an order watch across relay reconnects (#246 3/7) by [@grunch](https://github.com/grunch) in [#259](https://github.com/MostroP2P/mostro-cli/pull/259)
* Merge remote-tracking branch 'origin/main' into feat/order-watch-reconnect by [@grunch](https://github.com/grunch)
* feat: follow an order's mail and chat live (#246 2/7) by [@grunch](https://github.com/grunch) in [#258](https://github.com/MostroP2P/mostro-cli/pull/258)
* Merge remote-tracking branch 'origin/main' into feat/order-watch-subscription by [@grunch](https://github.com/grunch)
* feat: define the order watch contract (#246 1/7) by [@grunch](https://github.com/grunch) in [#257](https://github.com/MostroP2P/mostro-cli/pull/257)
* Merge remote-tracking branch 'origin/main' into feat/order-watch-contract by [@grunch](https://github.com/grunch)
* feat: add JSON output to order chat (#245 4/4) by [@grunch](https://github.com/grunch) in [#256](https://github.com/MostroP2P/mostro-cli/pull/256)
* Merge remote-tracking branch 'origin/main' into feat/order-chat-json by [@grunch](https://github.com/grunch)
* feat: add order chat (#245 3/4) by [@grunch](https://github.com/grunch) in [#255](https://github.com/MostroP2P/mostro-cli/pull/255)
* Merge remote-tracking branch 'origin/main' into feat/order-chat-command by [@grunch](https://github.com/grunch)
* feat: read, send and render the peer chat (#245 2/4) by [@grunch](https://github.com/grunch) in [#253](https://github.com/MostroP2P/mostro-cli/pull/253)
* Merge remote-tracking branch 'origin/main' into feat/order-chat-transport by [@grunch](https://github.com/grunch)
* feat: find the counterparty of an order (#245 1/4) by [@grunch](https://github.com/grunch) in [#252](https://github.com/MostroP2P/mostro-cli/pull/252)
* Merge remote-tracking branch 'origin/main' into feat/order-chat-counterparty by [@grunch](https://github.com/grunch)
* fix: apply the review fixes of the config file stack (#244 5/5) by [@grunch](https://github.com/grunch) in [#254](https://github.com/MostroP2P/mostro-cli/pull/254)
* feat: add config show (#244 4/5) by [@grunch](https://github.com/grunch) in [#251](https://github.com/MostroP2P/mostro-cli/pull/251)
* feat: add config set (#244 3/5) by [@grunch](https://github.com/grunch) in [#250](https://github.com/MostroP2P/mostro-cli/pull/250)
* feat: read the config file in the human-only commands (#244 2/5) by [@grunch](https://github.com/grunch) in [#249](https://github.com/MostroP2P/mostro-cli/pull/249)
* feat: read the node settings from the config file (#244 1/5) by [@grunch](https://github.com/grunch) in [#248](https://github.com/MostroP2P/mostro-cli/pull/248)
* Merge branch 'feat/fiat-sent-and-rate-steps' into feat/release-step by [@grunch](https://github.com/grunch)
* Merge branch 'feat/payment-failed-payload' into feat/fiat-sent-and-rate-steps by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-schema' into feat/payment-failed-payload by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-json' into feat/order-watch-schema by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-command' into feat/order-watch-json by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-command' into feat/order-watch-json by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-operation' into feat/order-watch-command by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-reconnect' into feat/order-watch-operation by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-subscription' into feat/order-watch-reconnect by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-watch-contract' into feat/order-watch-subscription by [@grunch](https://github.com/grunch)
* Merge branch 'feat/order-chat-json' into feat/order-watch-contract by [@grunch](https://github.com/grunch)
* feat: mine the proof of work the node advertises by [@grunch](https://github.com/grunch) in [#247](https://github.com/MostroP2P/mostro-cli/pull/247)
* docs: point programs and agents at the mostro-cli book by [@grunch](https://github.com/grunch) in [#241](https://github.com/MostroP2P/mostro-cli/pull/241)

### 🚜 Refactor


* absorb the rate grace batches in one place by [@grunch](https://github.com/grunch)

### 📚 Documentation


* recount the tests after the merged stacks by [@grunch](https://github.com/grunch)
* fix the test totals in the recount by [@grunch](https://github.com/grunch)
* recount the tests after merging main by [@grunch](https://github.com/grunch)
* recount the tests after merging main by [@grunch](https://github.com/grunch)
* recount the tests after merging main by [@grunch](https://github.com/grunch)
* recount the tests after merging main by [@grunch](https://github.com/grunch)
* record spec 018 unit 5 and its evidence by [@grunch](https://github.com/grunch)
* list every test target in tests/README.md by [@grunch](https://github.com/grunch)
* recount the test files in tests/README.md by [@grunch](https://github.com/grunch)
* limit the owner-only claims to Unix by [@grunch](https://github.com/grunch)
* name the config file in the flag docs by [@grunch](https://github.com/grunch)
* record spec 018 unit 4 and its evidence by [@grunch](https://github.com/grunch)
* describe config show by [@grunch](https://github.com/grunch)
* record spec 018 unit 3 and its evidence by [@grunch](https://github.com/grunch)
* describe config set by [@grunch](https://github.com/grunch)
* record spec 018 unit 2 and its evidence by [@grunch](https://github.com/grunch)
* describe the config file in the human-only commands by [@grunch](https://github.com/grunch)
* add spec 018 and its unit 1 evidence by [@grunch](https://github.com/grunch)
* describe the node settings file by [@grunch](https://github.com/grunch)
* say order sync saves every adopted order as made by [@grunch](https://github.com/grunch)
* list the rate grace deadline and truncated tests by [@grunch](https://github.com/grunch)
* say the binary reaches the watch now by [@grunch](https://github.com/grunch)
* describe the Gone future in the architecture by [@grunch](https://github.com/grunch)
* list the idle stdout and argument line tests by [@grunch](https://github.com/grunch)
* narrow the watch's error line to errors after parsing by [@grunch](https://github.com/grunch)
* quote the new deadline line in commands.md by [@grunch](https://github.com/grunch)
* correct the unit 3 sizes and record the CLOSED fix by [@grunch](https://github.com/grunch)
* correct the unit 2 window case and the moved test by [@grunch](https://github.com/grunch)
* list the spec 020 unit 1 tests in the test README by [@grunch](https://github.com/grunch)
* name uncertain in the shared status contract by [@grunch](https://github.com/grunch)
* qualify the cursor promise and the deadline line by [@grunch](https://github.com/grunch)
* record release in spec 021 by [@grunch](https://github.com/grunch)
* record fiat_sent and rate in spec 021 by [@grunch](https://github.com/grunch)
* add spec 021 and the v1 open-enum rule by [@grunch](https://github.com/grunch)
* record the order watch schema file by [@grunch](https://github.com/grunch)
* record order watch JSON output by [@grunch](https://github.com/grunch)
* record order watch in spec 020 by [@grunch](https://github.com/grunch)
* record the order watch operation by [@grunch](https://github.com/grunch)
* record the order watch reconnects by [@grunch](https://github.com/grunch)
* record the order watch subscription by [@grunch](https://github.com/grunch)
* add spec 020 and the contract evidence by [@grunch](https://github.com/grunch)
* amend the constitution for human streaming by [@grunch](https://github.com/grunch)
* correct two order chat records by [@grunch](https://github.com/grunch)
* record order chat JSON in spec 019 by [@grunch](https://github.com/grunch)
* record order chat in spec 019 by [@grunch](https://github.com/grunch)
* record the chat transport in spec 019 by [@grunch](https://github.com/grunch)
* add spec 019 and the lookup evidence by [@grunch](https://github.com/grunch)
* add spec 017 and its verification evidence by [@grunch](https://github.com/grunch)
* describe the advertised proof of work by [@grunch](https://github.com/grunch)
* say that schema discovery needs mostro-cli 0.17.0 by [@grunch](https://github.com/grunch)
* point programs and agents at the mostro-cli book by [@grunch](https://github.com/grunch)

### 🧪 Testing


* use an escape the order table never emits by [@grunch](https://github.com/grunch)
* check the rate grace never runs past the deadline by [@grunch](https://github.com/grunch)
* keep truncated from an empty batch at the end of the grace by [@grunch](https://github.com/grunch)
* stop an idle watch when its reader closes stdout by [@grunch](https://github.com/grunch)
* check the watch's own argument errors by [@grunch](https://github.com/grunch)
* expect the new deadline line from the binary by [@grunch](https://github.com/grunch)
* expect the deadline line to point to order info by [@grunch](https://github.com/grunch)
* judge a live replay cut by CLOSED for truncated by [@grunch](https://github.com/grunch)
* keep node ids when chat dated ahead fills the cap by [@grunch](https://github.com/grunch)
* check order watch against its schema file by [@grunch](https://github.com/grunch)

## Contributors
* [@grunch](https://github.com/grunch) made their contribution in [#267](https://github.com/MostroP2P/mostro-cli/pull/267)

**Full Changelog**: https://github.com/MostroP2P/mostro-cli/compare/v0.17.0...0.17.1

<!-- generated by git-cliff -->
