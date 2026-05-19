# Architectural Decision Records (ADRs)

ADRs capturam decisões arquiteturais significativas, o contexto que
as motivou, alternativas consideradas e consequências aceitas.

Formato adaptado de [Michael Nygard](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions).

## Índice

| ID | Título | Status | Sessão |
|---|---|---|---|
| [0001](./0001-domain-separated-hmac.md) | HMAC-SHA-256 com domain separation para `C₁` e `C₂` | Aceito | S1 |
| [0002](./0002-restricted-seed-id-alphabet.md) | Alfabeto restrito do `SeedId` excluindo `'0'` | Aceito | S1 |
| [0003](./0003-constant-time-comparison.md) | Comparação em tempo constante via `subtle` | Aceito | S1 |
| [0004](./0004-zero-io-core.md) | Núcleo isolado de I/O via traits injetadas | Aceito | S1 |
| [0005](./0005-ed25519-release-signing.md) | Ed25519 com pub key embarcada para assinatura de release | Aceito | S4 |
| [0006](./0006-separated-ffi-vs-uniffi.md) | Crates FFI e UniFFI separados | Aceito | S2 |
| [0007](./0007-dual-counter-mode.md) | Suporte dual a `T` sequencial e timestamp quantizado | Aceito | S6 |
| [0008](./0008-mtls-san-uri-identity.md) | Identidade da instituição via SAN URI no cert mTLS | Aceito | S8 |
| [0009](./0009-no-std-subset-crate.md) | Subset `no_std` em crate separada para microcontroladores | Aceito | S9 |
| [0010](./0010-no-panic-policy.md) | Política de não-pânico cobrindo FFI com `catch_unwind` | Aceito | S1 |
