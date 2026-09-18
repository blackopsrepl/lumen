# Changelog

All notable changes to this project will be documented in this file. See [commit-and-tag-version](https://github.com/absolute-version/commit-and-tag-version) for commit guidelines.

## [0.12.1](https://github.com/blackopsrepl/lumen/compare/v0.12.0...v0.12.1) (2026-09-18)

### Features

* **ratatui:** add a doctor example that diagnoses Lumen from inside Lumen ([f10fa34](https://github.com/blackopsrepl/lumen/commit/f10fa3447a67f1eda634dddf1181939459d35323))

### Bug Fixes

* **image:** copy the workspace member sources through the stub layer ([cb2514e](https://github.com/blackopsrepl/lumen/commit/cb2514efe57985f83c2d2674b3f1f005692c462e))

## [0.12.0](https://github.com/blackopsrepl/lumen/compare/v0.11.7...v0.12.0) (2026-09-18)

### Features

* **client:** ensure a ratatui session from the CLI ([d42cf4e](https://github.com/blackopsrepl/lumen/commit/d42cf4eb0cf66fef8303ba1a86c85bff2083d914))
* **config:** let the ratatui grid be overridden by environment ([26f8890](https://github.com/blackopsrepl/lumen/commit/26f88901eaa116fff843a3db0929fbc998bc2107))
* **ratatui:** add a Lumen backend and wire protocol for ratatui apps ([6b630a0](https://github.com/blackopsrepl/lumen/commit/6b630a03ae79630fc36201438ffd4c9f76355049))
* **ratatui:** add a trex example that runs under a Lumen session ([41b4275](https://github.com/blackopsrepl/lumen/commit/41b4275229b62a967ab7fe50dd252805fff6f1c3))
* **session:** supervise ratatui apps as a third session kind ([648c464](https://github.com/blackopsrepl/lumen/commit/648c464a2a09731867d0b414bd42c19f34a3f624))
* **viewer:** render and drive ratatui sessions as a cell grid ([4657619](https://github.com/blackopsrepl/lumen/commit/465761962c2eb284ef9acf34f29a0cc98d00f4b8))

## [0.11.7](https://github.com/blackopsrepl/lumen/compare/v0.11.6...v0.11.7) (2026-09-18)

### Features

* **viewer:** pan the streamed frame when control is not held ([a9829fa](https://github.com/blackopsrepl/lumen/commit/a9829faa7d9fb556129420c628e27aff8aa7857b))

## [0.11.6](https://github.com/blackopsrepl/lumen/compare/v0.11.5...v0.11.6) (2026-09-18)

### Features

* **client:** save feedback screenshots for the agent ([b20a7e2](https://github.com/blackopsrepl/lumen/commit/b20a7e2695639821e250fe5b8479f21d185bd465))
* **feedback:** attach a screenshot instead of region coordinates ([0fa5027](https://github.com/blackopsrepl/lumen/commit/0fa50276d8a1d66fddb954025b02e9d210328536))
* **viewer:** capture the annotated region as a PNG ([94cb877](https://github.com/blackopsrepl/lumen/commit/94cb8776c462b19abd44b7f54b03246c52d9f045))

## [0.11.5](https://github.com/blackopsrepl/lumen/compare/v0.11.4...v0.11.5) (2026-09-17)

### Bug Fixes

* **runtime:** use the FHS root for desktop projects ([9cb11ab](https://github.com/blackopsrepl/lumen/commit/9cb11ab5fa6d33136646df17a88b25cac2ea9569))

## [0.11.4](https://github.com/blackopsrepl/lumen/compare/v0.11.3...v0.11.4) (2026-09-17)

### Bug Fixes

* **session:** close viewers before backend teardown ([1dafe6e](https://github.com/blackopsrepl/lumen/commit/1dafe6ea13ae8d943f93a37b54badd853928e220))
* **view:** bound frames to active subscribers ([f60cfb3](https://github.com/blackopsrepl/lumen/commit/f60cfb371945b1f61581d2cb2cb4a9b73dd4fb0c))
* **viewer:** coalesce asynchronous frame decoding ([b84458e](https://github.com/blackopsrepl/lumen/commit/b84458e32a97e5e1287e876a279acbbc50115f5a))
* **view:** subscribe before desktop capture starts ([660174d](https://github.com/blackopsrepl/lumen/commit/660174d5aefe16228b618657dc76b60902e9813d))

## [0.11.3](https://github.com/blackopsrepl/lumen/compare/v0.11.2...v0.11.3) (2026-09-17)

### Bug Fixes

* **ci:** install complete Wayland desktop runtime ([2396a7b](https://github.com/blackopsrepl/lumen/commit/2396a7b1ab3a501aa95d1784e9ff80aa4c1df0ba))
* **container:** install Qt Quick QML modules ([ff12345](https://github.com/blackopsrepl/lumen/commit/ff12345ec1eaa82c3642c3157aefc43cf06f98c6))

## [0.11.2](https://github.com/blackopsrepl/lumen/compare/v0.11.1...v0.11.2) (2026-09-17)

### Bug Fixes

* **ci:** defer container runtime detection ([23abea8](https://github.com/blackopsrepl/lumen/commit/23abea8478bd36d34a9d28ee782cd216a6ebd7d6))
* **runtime:** expose host Quickshell projects ([db5ffe7](https://github.com/blackopsrepl/lumen/commit/db5ffe79d9ed5c19dfc4fd4f70c8b502928489b3))

## [0.11.1](https://github.com/blackopsrepl/lumen/compare/v0.11.0...v0.11.1) (2026-09-17)

### Bug Fixes

* **ci:** provide the desktop runtime in release gates ([8d676c8](https://github.com/blackopsrepl/lumen/commit/8d676c87939a0057b2c576f4888d98923b1883b8))

## [0.11.0](https://github.com/blackopsrepl/lumen/compare/v0.10.1...v0.11.0) (2026-09-17)


### Features

* **runtime:** support Docker as container runtime ([b840389](https://github.com/blackopsrepl/lumen/commit/b84038956fb603010c92f0c8e18185f557b93819))
* **session:** supervise Quickshell desktop sessions ([153afe2](https://github.com/blackopsrepl/lumen/commit/153afe2d91c1d9fc36641d17106bd8c00cbca74c))
* **viewer:** render Quickshell desktop sessions ([eaa9a2a](https://github.com/blackopsrepl/lumen/commit/eaa9a2a110e22253eb3baa823dbd5f9b456d4eb7))

## [0.10.1](https://github.com/blackopsrepl/lumen/compare/v0.10.0...v0.10.1) (2026-09-17)

### Bug Fixes

* **viewer:** freeze the annotation rectangle when the pointer is released ([82c31af](https://github.com/blackopsrepl/lumen/commit/82c31af5cdba4e948ebfbc23879f2429d7520e41))

## [0.10.0](https://github.com/blackopsrepl/lumen/compare/v0.9.0...v0.10.0) (2026-09-16)

### Features

* **http:** serve vendored IBM Plex fonts ([118a35d](https://github.com/blackopsrepl/lumen/commit/118a35d3067f5c16242b7a7969fb592468a427c0))
* **viewer:** restyle the console as a mono instrument panel ([6e5c7a3](https://github.com/blackopsrepl/lumen/commit/6e5c7a3018ae51caaed2317a26ee7010f642841d))

## 0.9.0 (2026-09-15)

### Features

* **agent:** report artifacts by absolute path and clarify the watcher contract ([30c13d6](https://github.com/blackopsrepl/lumen/commit/30c13d64fc082cd246b5810a3c2e165640fe20b9))
* **api:** add raw CDP, host policy, and an audit trail ([929690e](https://github.com/blackopsrepl/lumen/commit/929690ea6e783a6b3ea370ebc9266577bc555676))
* **api:** expose the session and viewport control plane ([ffb25df](https://github.com/blackopsrepl/lumen/commit/ffb25df91d533c41cb1a5e94314923e2380df28b))
* **api:** stop and forget a session ([d705b29](https://github.com/blackopsrepl/lumen/commit/d705b295b5bfd38b7485386a4549bdf6bc01923c))
* **capabilities:** add tabs, screenshot, visibility, and page scale ([a1f1640](https://github.com/blackopsrepl/lumen/commit/a1f1640c2daf8061ab5a602863766125def9f508))
* **cdp:** drive isolated Chromium sessions over CDP ([888ccb6](https://github.com/blackopsrepl/lumen/commit/888ccb695876061eebb76febe8c13e42d695a89d))
* **cli:** add the lumen control client ([692cc9c](https://github.com/blackopsrepl/lumen/commit/692cc9c392469407809374a773a534848a9e599f))
* **feedback:** keep human annotations in a durable inbox ([bea3d97](https://github.com/blackopsrepl/lumen/commit/bea3d975ed5510a104c5153434ff1322a1ff1c7c))
* **http:** reject non-loopback Host and Origin ([5751374](https://github.com/blackopsrepl/lumen/commit/5751374e162931616014d1169e7d1394aad918bc))
* **lumen:** scaffold the single-binary service ([5e5036b](https://github.com/blackopsrepl/lumen/commit/5e5036bf157c9230f9bf91c60277bfc02e30c38d))
* **sessions:** distinguish agent-owned sessions from manual ones ([f886200](https://github.com/blackopsrepl/lumen/commit/f886200d20ab51029c5a5f6fb318276abcd47bbb))
* **supervisor:** own ephemeral browser profile lifecycle ([d67f157](https://github.com/blackopsrepl/lumen/commit/d67f157c1819d2e6d61a58f043f1d9adee0ee135))
* **ui:** rebuild the viewer ([a8ee20c](https://github.com/blackopsrepl/lumen/commit/a8ee20c2ee364bd68496a3816bec6dfe33bb4fe4))
* **view:** stream the page to a live, controllable viewer ([2dd3d8a](https://github.com/blackopsrepl/lumen/commit/2dd3d8ae2b852237e1b4eb66e91fac13309a2d29))

### Bug Fixes

* **agent:** bind the CLI to the service's endpoint and own artifacts per session ([0814eeb](https://github.com/blackopsrepl/lumen/commit/0814eebae0d7a601922d1fdf2cda2c49cce1bcb5))
* **agent:** initialize the CLI workspace at the single choke point ([a3fc2b6](https://github.com/blackopsrepl/lumen/commit/a3fc2b66372fee53922257721d0076996eec64a3))
* **agent:** make close end the session, not just unbind the client ([8b1ea21](https://github.com/blackopsrepl/lumen/commit/8b1ea214ee83f9106ca63c95155970880db5442e))
* **agent:** make close report the real session outcome ([ca7f58f](https://github.com/blackopsrepl/lumen/commit/ca7f58fb7559500e60fc7cabe3b8346d1334546a))
* **agent:** reject unsafe session names before filesystem use ([fa82f6c](https://github.com/blackopsrepl/lumen/commit/fa82f6c79c5ed16d41ea85090146f28535752d01))
* **api:** answer an invalid session name with 400, not 500 ([ce9d319](https://github.com/blackopsrepl/lumen/commit/ce9d3194d3c05d0c9bc842baa507a590fc88e318))
* **audit:** bound the audit trail ([c0d088b](https://github.com/blackopsrepl/lumen/commit/c0d088b8ee81798d960cabaa0fc7008da5ae9718))
* **cdp:** bound every step of a raw CDP command ([47cab35](https://github.com/blackopsrepl/lumen/commit/47cab350d18811db1a7ec44ff4b95fd4149f35bc))
* **cdp:** correlate a blocked navigation with the navigation itself ([a79eefe](https://github.com/blackopsrepl/lumen/commit/a79eefe4be9888242b20e1098eb72f352bd8f8c4))
* **cdp:** hold the managed page for the whole of every operation ([39d67b2](https://github.com/blackopsrepl/lumen/commit/39d67b26b835409d968c6cba82eafe695f81c128))
* **cdp:** keep listing tabs while a target is disappearing ([3797c67](https://github.com/blackopsrepl/lumen/commit/3797c67b5472ae229cab04f7fce6e7eb192d423e))
* **cdp:** keep one shared page per browser ([2e730e4](https://github.com/blackopsrepl/lumen/commit/2e730e4d3886052f219def0a6cbe401123ec4d31))
* **cdp:** never report or address a target that is closing ([2d49d5e](https://github.com/blackopsrepl/lumen/commit/2d49d5e41d57dbc29ac5a9b7614c67a86269c580))
* **cdp:** recover when the managed tab disappears ([ecca0e7](https://github.com/blackopsrepl/lumen/commit/ecca0e7e85dacf2494b1b0e120e2652a4b164e3f))
* **config:** report a bad bind host instead of panicking ([8c9e44b](https://github.com/blackopsrepl/lumen/commit/8c9e44b35541d61dde25b5bd99d40df40bad8975))
* **feedback:** acknowledge exactly the notes that were read ([2301bf5](https://github.com/blackopsrepl/lumen/commit/2301bf5d55a0ddb7348a48cfabe77d6664851a80))
* **ops:** bound the drain, not the server's lifetime ([e1cc713](https://github.com/blackopsrepl/lumen/commit/e1cc7136bb1be49e9e51e20c63119c673c4630d0))
* **ops:** converge the running service onto the built revision ([7d001d3](https://github.com/blackopsrepl/lumen/commit/7d001d3a6a968d0fb469a335e20a47ad29cf0530))
* **ops:** drain the http plane before stopping browsers ([4e94fdd](https://github.com/blackopsrepl/lumen/commit/4e94fddb401021b9122e1724e75a31d3be6d0f97))
* **ops:** make helper scripts report what actually happened ([a1a0473](https://github.com/blackopsrepl/lumen/commit/a1a047362a87b841b8fd4ad31abf06720323722f))
* **ops:** make host deployment and smoke checks configurable ([9716e17](https://github.com/blackopsrepl/lumen/commit/9716e17354ab85dbdca4446d2d2de1be0071543d))
* **ops:** own the container log level with LUMEN_LOG ([9728ecc](https://github.com/blackopsrepl/lumen/commit/9728ecccc667efe6b954915990bb2431cc01635a))
* **ops:** stop every agent browser on service shutdown ([30d86d7](https://github.com/blackopsrepl/lumen/commit/30d86d76ac06098eba64190d0b7b2b21b4e2d698))
* run the full clippy gate in pre-commit ([0053d15](https://github.com/blackopsrepl/lumen/commit/0053d151d436ccee284b95406ee9538cbfd590d0))
* **runtime:** preserve browser and navigation invariants ([aa1e3e3](https://github.com/blackopsrepl/lumen/commit/aa1e3e3e2368d886a3600919a303fd1a8f66d79c))
* **skill:** drop mid-scalar colon that broke frontmatter parsing ([c9ab199](https://github.com/blackopsrepl/lumen/commit/c9ab1994f6ed645b8e57a6b768cef45cb6817d71))
* **supervisor:** prove the profile root is Lumen's before clearing it ([de43d1d](https://github.com/blackopsrepl/lumen/commit/de43d1d3a412947947085521bf861fc2e7e0ac44))
* **supervisor:** stop the whole browser process group on teardown ([bf115e1](https://github.com/blackopsrepl/lumen/commit/bf115e189f18f9a394f621b708c2fd1150e7dc0f))
* **test:** give every e2e run its own port and state ([c79b2df](https://github.com/blackopsrepl/lumen/commit/c79b2dfa18e19459bff842557a21f692b0c597fb))
* **view:** adopt tabs the browser opens itself ([22cca11](https://github.com/blackopsrepl/lumen/commit/22cca11397eaf9ad0f91ee410a8f390431eb4f05))
* **viewer:** recover cleanly across session and input changes ([c93ab48](https://github.com/blackopsrepl/lumen/commit/c93ab48fe355dbb0a58273f6dff019e1c62631c3))
* **view:** paint joining viewers with the last screencast frame ([e91b259](https://github.com/blackopsrepl/lumen/commit/e91b2598e3a50c7cef84b7af890b0a024b3a3c7d))
