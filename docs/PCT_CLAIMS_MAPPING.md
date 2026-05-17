# Mapeamento das reivindicações técnicas do pedido PCT ao código

Este documento é o **artefato de evidência** que liga cada cláusula
técnica do pedido PCT à(s) linha(s) de código que a implementam e ao(s)
teste(s) que a validam. Destinado ao examinador técnico e a auditores
independentes que precisam confirmar que a SDK efetivamente realiza
o que o pedido descreve.

Estrutura por bloco: cada reivindicação técnica → mecanismo
correspondente no código → caminho do arquivo:linha → teste que
exercita o caminho.

## 1. Par criptográfico atômico

**Reivindicação.** O recebedor gera, offline, um par `(C₁, C₂)`
derivado de função criptográfica unidirecional sobre semente `S` e
parâmetro variável `T`, com `C₂` encadeado a `C₁`.

| Componente | Local |
|---|---|
| HMAC-SHA-256 como função unidirecional | `crates/mcpix-core/src/crypto.rs:39-50` |
| Derivação de `C₁` | `crates/mcpix-core/src/crypto.rs:65` |
| Derivação de `C₂` com encadeamento explícito | `crates/mcpix-core/src/crypto.rs:80-87` |
| Domain separation `dom_c1` e `dom_c2` | `crates/mcpix-core/src/crypto.rs:55-56` |
| Test: determinismo | `crates/mcpix-core/src/crypto.rs::derive_pair_is_deterministic` |
| Property: encadeamento bit-exato | `crates/mcpix-core/tests/properties.rs::c2_recoverable_from_c1` |

## 2. Substituição institucional pelo banco do pagador

**Reivindicação.** O banco do pagador, recebendo o instrumento
contendo `C₁` e consultando `S` no banco do recebedor por canal
inter-institucional, reconstrói `C₂` mediante aplicação da mesma
função criptográfica — sem qualquer canal direto com o recebedor.

| Componente | Local |
|---|---|
| Função pura de recomposição | `crates/mcpix-core/src/state.rs:80-86` (`apply_recover_c2`) |
| Mock do banco do pagador | `crates/mcpix-bank-payer-mock/src/lib.rs:59-82` (`process_payment`) |
| Consulta inter-institucional (mock) | `crates/mcpix-bank-receiver/src/lib.rs::lookup_seed` |
| Consulta inter-institucional (REST + mTLS) | `crates/mcpix-bank-receiver/src/http_client.rs::lookup_seed` |
| Test: bit-exatidão entre lados | `mcpix-bank-payer-mock::payer_bank_recovers_same_c2_as_receiver` |
| Test: e2e HTTP real | `mcpix-bank-receiver::tests::full_protocol_through_http` |

## 3. Identificação do esquema na faixa de transporte

**Reivindicação.** O instrumento de cobrança é uma string alfanumérica
de tamanho compatível com `[a-zA-Z0-9]{26,35}` cujo prefixo sinaliza
ao banco do pagador a presença do mecanismo.

| Componente | Local |
|---|---|
| Constante de prefixo `PIXOFFv1` | `crates/mcpix-core/src/transport_field.rs:23` |
| Layout posicional (35 chars fixos) | `crates/mcpix-core/src/transport_field.rs:24` |
| Função de triagem `is_protocol_field` | `crates/mcpix-core/src/transport_field.rs::is_protocol_field` |
| Triagem no banco do pagador | `crates/mcpix-bank-payer-mock/src/lib.rs:63-65` |
| Test: round-trip encode/parse | `crates/mcpix-core/src/transport_field.rs::round_trip_preserves_fields` |
| Property: round-trip universal | `crates/mcpix-core/tests/properties.rs::encode_parse_roundtrip` |

## 4. Comparação local em tempo constante

**Reivindicação.** A comparação entre `C₂` retido e `C₂` apresentado
opera em tempo independente do conteúdo dos códigos, defendendo-se
contra ataques de canal lateral por timing.

| Componente | Local |
|---|---|
| `verify_c2` via `subtle::ConstantTimeEq` | `crates/mcpix-core/src/crypto.rs:121` |
| Wrapper de validação local | `crates/mcpix-core/src/state.rs:64-76` (`apply_validate_receipt`) |
| Comentário técnico inline justificando | `crates/mcpix-core/src/crypto.rs:115-121` |
| Test: equivalência funcional | `crates/mcpix-core/src/crypto.rs::verify_accepts_match` + `verify_rejects_mismatch` |
| Property: equivalência sobre todos os inputs | `crates/mcpix-core/tests/properties.rs::verify_c2_equiv_to_equality` |

## 5. Modos de parâmetro variável `T`

**Reivindicação.** O parâmetro `T` pode ser instanciado como contador
unidirecional sequencial **ou** como timestamp quantizado, ambos
preservando a propriedade de unidirecionalidade.

| Componente | Local |
|---|---|
| Trait `Counter` | `crates/mcpix-core/src/traits.rs::Counter` |
| Impl sequencial | `crates/mcpix-receiver-sdk/src/monotonic_counter.rs::InMemoryCounter` |
| Impl timestamp quantizado | `crates/mcpix-receiver-sdk/src/timestamp_counter.rs::TimestampQuantizedCounter` |
| Enforçamento de monotonia | `timestamp_counter.rs:71-84` |
| Anti-colisão dentro de janela | `timestamp_counter.rs:78` |
| Anti-rollback de relógio | `timestamp_counter.rs:81` |
| Tolerância de drift no banco | `mcpix-bank-payer-mock/src/lib.rs::process_payment_windowed` |
| Tests dedicados | `timestamp_counter::tests::*` (5 testes) |
| ADR justificando | [adr/0007-dual-counter-mode.md](./adr/0007-dual-counter-mode.md) |

## 6. Substituição futura por hardware seguro

**Reivindicação.** A arquitetura admite substituição transparente da
custódia do material da semente por Secure Enclave / HSM /
TPM / Element Seguro embarcado.

| Componente | Local |
|---|---|
| Trait `SeedStore` (interface única) | `crates/mcpix-core/src/traits.rs::SeedStore` |
| Impl in-memory (demo) | `crates/mcpix-receiver-sdk/src/memory_store.rs` |
| Impl SQLite (próximo passo) | `crates/mcpix-receiver-sdk/src/sqlite_store.rs` |
| Comentário inline em `Seed` apontando substituição | `crates/mcpix-core/src/types.rs:33-37` |
| ADR de isolamento de I/O | [adr/0004-zero-io-core.md](./adr/0004-zero-io-core.md) |
| Zeroização do material em memória | `Seed: ZeroizeOnDrop` em `types.rs:30` |

## 7. Defesa contra reuso (replay)

**Reivindicação.** O recebedor recusa segunda apresentação de um
mesmo comprovante.

| Componente | Local |
|---|---|
| Flag `consumed` no retained | `crates/mcpix-core/src/types.rs::RetainedReceipt::consumed` |
| Marcação atômica após validação | `crates/mcpix-receiver-sdk/src/lib.rs:90-106` |
| Outcome `Replay` distinto de `Mismatch` | `crates/mcpix-core/src/state.rs::ValidationOutcome::Replay` |
| Tests | `state::tests::validation_rejects_replay`, `receiver_sdk::tests::replay_is_rejected` |

## 8. Integridade do binário e cadeia de confiança

**Reivindicação.** O SDK detecta adulteração do próprio binário
(substituição ou patching) e verifica a procedência por assinatura
digital.

| Componente | Local |
|---|---|
| SHA-256 self-check | `crates/mcpix-core/src/integrity.rs::verify_bytes` |
| Hash esperado carimbado em build | `MCPIX_EXPECTED_SHA256` via `option_env!` em `integrity_runtime.rs:19` |
| Verificação Ed25519 do manifesto | `crates/mcpix-core/src/signature.rs::verify_combined` |
| Chave pública embarcada | `crates/mcpix-core/trusted_keys/release.pub` (32 bytes) + `signature.rs:32` |
| Localização adjacente ao binário | `crates/mcpix-receiver-sdk/src/integrity_runtime.rs::locate_sums` |
| Tests contra artefato real | `crates/mcpix-receiver-sdk/tests/integrity_against_dist.rs` (5 testes) |

## 9. Portabilidade para microcontroladores

**Reivindicação.** O componente recebedor é portável para hardware
embarcado de baixo recurso, mantendo o algoritmo bit-exato com a
implementação para servidor.

| Componente | Local |
|---|---|
| Crate `no_std` sem alloc | `crates/mcpix-embed/` |
| Algoritmo replicado | `crates/mcpix-embed/src/crypto.rs::derive_pair` |
| Tipos em stack | `crates/mcpix-embed/src/types.rs` |
| QR encoder embarcado | `crates/mcpix-embed/src/qr.rs::charge_qr` |
| Cross-validation contra host | `crates/mcpix-embed/tests/cross_validate.rs` (3 testes) |
| Binário Cortex-M4F demo | `embedded/src/main.rs` (16.5 KB .text, 0 BSS) |

## 10. Não-pânico do SDK na aplicação hospedeira

**Reivindicação.** O SDK nunca capota a aplicação que o consome.

| Componente | Local |
|---|---|
| Política em camada Rust | `Result<_, McpixError>` em todas funções públicas |
| `catch_unwind` na FFI | `crates/mcpix-ffi/src/handle.rs::guard`, `guard_mut` |
| Status `Panic` mapeado | `crates/mcpix-ffi/src/error.rs::McpixStatus::Panic` |
| Property: parser nunca panica | `tests/properties.rs::parse_never_panics_on_arbitrary_strings` |
| Fuzz: 25M+ inputs adversariais sem crash | `fuzz/fuzz_targets/fuzz_transport_parse.rs` |

## Resumo numérico

| Reivindicação | Mecanismos | Tests dedicados | ADR |
|---|---:|---:|---:|
| 1. Par atômico | 5 | 2 + 14 properties | 0001 |
| 2. Substituição institucional | 6 | 2 | — |
| 3. Triagem por prefixo | 4 | 2 + property | — |
| 4. Tempo constante | 4 | 2 + property | 0003 |
| 5. Modos de `T` | 7 | 5 | 0007 |
| 6. Custódia substituível | 4 | — | 0004 |
| 7. Defesa replay | 4 | 2 | — |
| 8. Integridade do binário | 6 | 5 | 0005 |
| 9. Portabilidade MCU | 6 | 4 (3 cross-val + 1 unit) | 0009 |
| 10. Não-pânico | 5 | 1 property + 1 fuzz target | 0010 |
