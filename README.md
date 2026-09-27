# Withdrawn: Fact client helpers

**Do not add `univers-aip-lib-fact-client@0.1.0` to new consumers.**
This package was incorrectly classified as a generic library. Fact contracts
already belong to the existing C0 World contract: `FactStore`, `DataFact`, and
`FactQuery`. Fact acceptance, persistence, canonical identity and authoritative
idempotency/write receipts belong to the World implementation.

The extracted `assert_fact_for_organization` helper skipped the World write when
its latest query returned an equal value, then locally constructed a
`Skipped`/`replayed` receipt. That observation is not a durable World commit or
replay and must not be presented as one. Depending only on public Ports did not
make this domain behavior a generic technical mechanism.

Consumers should pass their business inputs through the existing World Ports
and preserve the actual World-issued outcome/receipt. Application-specific
read selection can remain with the consumer; it must not become a second Fact
authority. No replacement Fact contract or renamed client library is planned.

Resource removed this dependency in develop commit
`37b20d176a315525ec7f1a6d19946efc394be19b`. The 0.1.0 source and published archive
remain historical evidence; their passing tests do not establish a valid
architectural boundary. Registry yanking is used where supported to stop new
resolution while preserving existing lockfile reproducibility.
