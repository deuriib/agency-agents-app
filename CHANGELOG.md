# Changelog

## [0.9.2](https://github.com/deuriib/agency-agents-app/compare/v0.9.1...v0.9.2) (2026-11-03)


### Refactors

* **rules:** use {slug} and the agent tool ids for rule harnesses ([#78](https://github.com/deuriib/agency-agents-app/issues/78)) ([b6b13ee](https://github.com/deuriib/agency-agents-app/commit/b6b13ee58b30701ee9f4f14bfecdc092ecfb6c56))

## [0.9.1](https://github.com/deuriib/agency-agents-app/compare/v0.9.0...v0.9.1) (2026-11-03)


### Bug Fixes

* **release:** take the Apple identity from the environment ([#74](https://github.com/deuriib/agency-agents-app/issues/74)) ([7f58af3](https://github.com/deuriib/agency-agents-app/commit/7f58af3c74b3aafbbc30eb67c884873e04848d60))

## [0.9.0](https://github.com/deuriib/agency-agents-app/compare/v0.8.4...v0.9.0) (2026-11-03)


### Features

* **rules:** integrate agency-rules with native compilation and ledger tracking ([#61](https://github.com/deuriib/agency-agents-app/issues/61)) ([1ab9453](https://github.com/deuriib/agency-agents-app/commit/1ab9453035d7a333a4eecd99657bc5a68bffc082))

## [0.8.4](https://github.com/deuriib/agency-agents-app/compare/v0.8.3...v0.8.4) (2026-11-03)


### Bug Fixes

* **catalog:** carry strategy/ and divisions.json in snapshot provisioning ([#59](https://github.com/deuriib/agency-agents-app/issues/59)) ([3355730](https://github.com/deuriib/agency-agents-app/commit/3355730f5b10629cfabd355d6063e34242e42950))

## [0.8.3](https://github.com/deuriib/agency-agents-app/compare/v0.8.2...v0.8.3) (2026-11-02)


### Refactors

* **registry:** read IMPLEMENTED_FORMATS from a shared JSON ([#54](https://github.com/deuriib/agency-agents-app/issues/54)) ([fa9e8c6](https://github.com/deuriib/agency-agents-app/commit/fa9e8c69f07948c1a421600d7cd762436bfc1ce2))

## [0.8.2](https://github.com/deuriib/agency-agents-app/compare/v0.8.1...v0.8.2) (2026-11-02)


### Bug Fixes

* **install:** refuse writes to unported tools and stop advertising them ([#51](https://github.com/deuriib/agency-agents-app/issues/51)) ([651f249](https://github.com/deuriib/agency-agents-app/commit/651f249091158f111dc70eab43357271f00c452c))

## [0.8.1](https://github.com/deuriib/agency-agents-app/compare/v0.8.0...v0.8.1) (2026-11-02)


### Bug Fixes

* **updater:** rotate the signing key for CI-only releases ([#42](https://github.com/deuriib/agency-agents-app/issues/42)) ([98c3e52](https://github.com/deuriib/agency-agents-app/commit/98c3e5248690d667e7f2975f5fac6156fc294979))

## [0.8.0](https://github.com/deuriib/agency-agents-app/compare/v0.7.1...v0.8.0) (2026-11-02)


### Features

* **updates:** make updates installable and add release channels ([#38](https://github.com/deuriib/agency-agents-app/issues/38)) ([a453bd2](https://github.com/deuriib/agency-agents-app/commit/a453bd2fba4c04f04257a652fa4ae180ad784c71))

## [0.7.1](https://github.com/deuriib/agency-agents-app/compare/v0.7.0...v0.7.1) (2026-11-02)


### Bug Fixes

* **ci:** package the Linux AppImage under umask 0022 ([35925ba](https://github.com/deuriib/agency-agents-app/commit/35925bad1a9a9750e07eda627524866be6f30fb6))
* **ci:** package the Linux AppImage under umask 0022 ([689343d](https://github.com/deuriib/agency-agents-app/commit/689343d840c749969fe0e9cfdb50b6e49ad60c81))

## [0.7.0](https://github.com/deuriib/agency-agents-app/compare/v0.6.0...v0.7.0) (2026-11-02)


### Features

* **render:** declare per-role antigravity tool lists ([d27bd7e](https://github.com/deuriib/agency-agents-app/commit/d27bd7ede14f35c576cb428fdf2a8371242ecbb1))
* **render:** declare per-role antigravity tool lists ([8cb426e](https://github.com/deuriib/agency-agents-app/commit/8cb426e85e4a4776af70fac43a631afba74f0746)), closes [#26](https://github.com/deuriib/agency-agents-app/issues/26)
* **render:** promote agents-orchestrator to primary agent ([1fcbc45](https://github.com/deuriib/agency-agents-app/commit/1fcbc45b248c7062ce02add9864b56dca2b62fb1))
* **render:** promote agents-orchestrator to primary agent ([4743c6b](https://github.com/deuriib/agency-agents-app/commit/4743c6b33294f9d3a8ab9ee847ebd75f521b735f)), closes [#22](https://github.com/deuriib/agency-agents-app/issues/22)
* **tools:** validate tools.json against a JSON schema ([98eee4c](https://github.com/deuriib/agency-agents-app/commit/98eee4cd4e41333929e6ba224d69f8e9e54c49c2))
* **tools:** validate tools.json against a JSON schema ([f0e1127](https://github.com/deuriib/agency-agents-app/commit/f0e11272d6d338c5a98bac2a5d20096437b1503c)), closes [#30](https://github.com/deuriib/agency-agents-app/issues/30)


### Bug Fixes

* **scripts:** resolve bump-version root from script path ([cfba43a](https://github.com/deuriib/agency-agents-app/commit/cfba43a5247dec13730a5228e2006410f781108d))
* **tools:** fix complete roster of tools detectDir paths ([8055cfc](https://github.com/deuriib/agency-agents-app/commit/8055cfcec5c04f65fd4ab317a1a744851ecb9f09))
* **tools:** gemini-cli detect dirPath ([c05e2be](https://github.com/deuriib/agency-agents-app/commit/c05e2beb0c4bbee3133f41eabc465f7771ec4373))
* **tools:** omp, pi-agent and antigavity detect dirPaths ([12d1bcc](https://github.com/deuriib/agency-agents-app/commit/12d1bccb550a62249602108dc092749351a6b051))


### Refactors

* apply rustfmt and clear clippy warnings ([26619d5](https://github.com/deuriib/agency-agents-app/commit/26619d500ab2184b6c51b1642bd6516e7e7bbcbc))

## Changelog

All notable changes to this project are documented here. Entries are generated
from [Conventional Commits](https://www.conventionalcommits.org/) by
[release-please](https://github.com/googleapis/release-please) when a release
is cut. Do not edit this file by hand — write narrative highlights in
`docs/release-notes/<version>.md` instead.
