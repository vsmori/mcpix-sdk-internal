# Threat model

## 1. Escopo

Este documento enumera ataques considerados, atores adversariais, e
mecanismos de defesa implementados na SDK. Adota convenção
**STRIDE-like** mas organizada por superfície (transporte, runtime,
binário, persistência, hardware).

## 2. Atores

| Ator | Capacidades assumidas |
|---|---|
| **Eavesdropper de rede** | observa tráfego entre bancos sem capacidade de modificar |
| **Man-in-the-middle ativo** | observa e modifica tráfego entre bancos |
| **Atacante local no recebedor** | acesso ao filesystem do dispositivo, pode substituir o binário do SDK |
| **Pagador desonesto** | pode tentar replay do comprovante, falsificar campo de transporte |
| **Banco do pagador comprometido** | autenticidade da consulta inter-bancária |
| **Atacante físico** | extração de flash, leitura JTAG (escopo limitado) |
| **Atacante de cadeia de build** | comprometimento do CI ou da chave de release (parcialmente coberto) |

## 3. Ativos

| Ativo | Local | Sensibilidade |
|---|---|---|
| Semente `S` | dispositivo do recebedor + banco do recebedor | **crítica** |
| `C₂` retido | dispositivo do recebedor (até consumido) | alta |
| Contador `T` | dispositivo do recebedor (modo sequencial) | média |
| Chave privada de release | secret de CI + backup offline | **crítica** |
| Cert privado da instituição (mTLS) | banco | alta |
| Binário do SDK | dispositivo do recebedor | média |
| Conteúdo de `C₁` | público no instrumento | nula |

## 4. Superfície de protocolo

### 4.1 Falsificação do instrumento de cobrança

**Ataque.** Atacante fabrica string `PIXOFFv1<seed_id_alvo><C₁_arbitrário>`
e passa ao pagador.

**Defesa.** Banco do pagador recompõe `C₂` a partir do `S` legítimo
(via lookup autenticado) e do `C₁` apresentado. Se `C₁` foi forjado
sem conhecer `S`, o `C₂` recomposto não bate com nenhum retained no
recebedor — `validate_receipt` retorna `Mismatch`. Atacante não tira
proveito porque o BIP não é emitido.

Cobertura de teste: `state::tests::validation_rejects_wrong_c2`.

### 4.2 Forjamento do comprovante

**Ataque.** Pagador desonesto fabrica `C₂` aleatório e apresenta ao
recebedor.

**Defesa.** `verify_c2` em tempo constante; espaço de busca cego
`32^11 ≈ 2^55`. Em modo sequencial, defesa de replay nega
re-apresentação após primeira `Valid`. Em modo timestamp quantizado,
`C₂` válido em janela `T` não vale em janela `T+1`.

Cobertura: `crypto::tests::verify_rejects_mismatch`,
`crypto::tests::different_counter_yields_different_pair`, propriedade
`random_bytes_never_verify`.

### 4.3 Replay do comprovante

**Ataque.** Pagador captura `C₂` válido emitido pelo banco e apresenta
ao recebedor duas vezes (ex. tentando aceitar mesma cobrança em duas
máquinas distintas do mesmo recebedor).

**Defesa.** `RetainedReceipt::consumed` é marcado *atomicamente*
antes do retorno `Valid`. Segunda apresentação encontra `consumed
== true` → `ValidationOutcome::Replay`. O store é responsável pela
atomicidade (in-memory usa `Mutex`; SQLite usa transação implícita).

Cobertura: `state::tests::validation_rejects_replay`,
`receiver_sdk::tests::replay_is_rejected`.

### 4.4 Timing attack na comparação

**Ataque.** Adversário com canal de timing (medição de tempo até o
BIP, observação de I/O do dispositivo) tenta recuperar `C₂` esperado
byte-a-byte através de `==` ingênua.

**Defesa.** `subtle::ConstantTimeEq`. Sequência de instruções
independente do conteúdo. Cobertura em [adr/0003](./adr/0003-constant-time-comparison.md).

### 4.5 Colisão de contador

**Ataque.** Atacante força duas cobranças no mesmo quantum (modo
timestamp) para gerar dois `C₁` idênticos com `amount` diferente —
a primeira é então sobrescrita silenciosamente.

**Defesa.** `TimestampQuantizedCounter::next` enforça monotonia por
SeedId. Segunda chamada no mesmo quantum retorna
`McpixError::CounterCollision { window_seconds }`. Aplicação
recebedora deve tratar e tentar novamente no próximo quantum.

Cobertura: `timestamp_counter::tests::same_window_call_is_rejected`.

### 4.6 Rollback de relógio

**Ataque.** Atacante com acesso ao dispositivo recua o relógio para
reusar um `T` antigo cujo `C₂` foi observado e capturado.

**Defesa.** `TimestampQuantizedCounter` mantém `last_issued` por
SeedId. Cada `next()` exige `T_now > last_issued`; retorno é
`CounterRollback { last, now }` caso contrário.

Cobertura: `timestamp_counter::tests::clock_rollback_is_rejected`.

## 5. Superfície de binário

### 5.1 Substituição do `.so`/`.dll`/`.dylib`

**Ataque.** Atacante com acesso ao filesystem substitui a biblioteca
nativa do SDK por uma versão modificada que muda o `C₂` derivado
(p. ex., zerando partes do hash) para forjar comprovantes.

**Defesa em camadas:**

1. **SHA-256 self-check** (S3): hash do binário esperado é carimbado
   em build time via `MCPIX_EXPECTED_SHA256`. `verify_self()` compara.
2. **Manifesto assinado** (S4): `SHA256SUMS` listando todos os hashes
   do release é assinado por chave Ed25519 privada (em secret de CI).
   Chave pública está embarcada no binário; substituir o `.so` exige
   também forjar uma assinatura, o que requer a chave privada.

Cobertura: `tests/integrity_against_dist.rs` (5 testes contra
artefatos reais).

### 5.2 Patch in-place do binário (.text)

**Ataque.** Atacante modifica bytes da seção `.text` do binário
mantendo o tamanho (ex. injeta backdoor que vaza `S`).

**Defesa.** Mesma da §5.1 — hash detecta qualquer flip de byte.
Validado em `verify_detects_tampering_in_real_artifact` que flipa 1
byte e confirma `Tampered`.

### 5.3 Comprometimento do CI

**Ataque.** Atacante compromete o pipeline GitHub Actions, sub-roteia
a build para inserir backdoor mas mantém a assinatura válida (porque
controla o secret).

**Defesa.** *Endereçada via SLSA L3.* O job `provenance` em
`release.yml` invoca o reusable workflow oficial
`slsa-framework/slsa-github-generator` para emitir
`mcpix-sdk.intoto.jsonl` em runner separado do build, com:

- **Assinatura keyless** via Sigstore Fulcio — cert X.509 efêmero
  ligado ao OIDC token do GitHub Actions; não há chave persistente.
- **Transparency log** Rekor — toda emissão fica visível publicamente;
  manipulações posteriores requerem comprometer o log.
- **Predicate slsa-provenance v1** ligando o digest do artefato ao
  commit fonte, workflow path, ref e materials.

Consumidor verifica antes de carregar o `.so`:

```bash
slsa-verifier verify-artifact \
  --provenance-path mcpix-sdk.intoto.jsonl \
  --source-uri github.com/vsmori/mcpix-sdk-internal \
  --source-tag v1.0.0 \
  ./libmcpix_ffi.so
```

Detalhes operacionais e script `scripts/verify-release.sh` em
[`SLSA.md`](SLSA.md). Atacante que comprometa um runner não consegue
forjar o OIDC token sem comprometer o próprio GitHub IdP — atendendo
ao requisito SLSA L3 de "provenance não-falsificável". *Resíduo:* L4
(build hermético reproduzível) fica como roadmap.

### 5.4 Hijacking de dependência (LD_PRELOAD, DLL search order)

**Ataque.** Atacante injeta DLL/SO maliciosa antes da legítima no
caminho de carga do processo hospedeiro.

**Defesa.** *Não coberta nesta entrega.* Mitigação requer remote
attestation / TEE (Secure Enclave iOS, StrongBox Android) e está fora
do escopo da SDK pura — depende de capacidades da plataforma.

## 6. Superfície de transporte (inter-bancos)

### 6.1 Eavesdropping na consulta de semente

**Ataque.** Adversário escuta GET `/v1/seeds/{seed_id}` e captura `S`.

**Defesa.** TLS 1.3 obrigatório quando feature `mtls` ativa. Material
da semente trafega em base64 dentro do canal cifrado.

### 6.2 Impersonação do banco do pagador

**Ataque.** Adversário se passa por banco do pagador e consulta
sementes que não devia.

**Defesa.** mTLS — banco do recebedor verifica cert do cliente contra
CA federada. Cliente sem cert válido falha no handshake (validado
em `mtls_rejects_client_without_cert`).

### 6.3 Impersonação do banco do recebedor

**Ataque.** Atacante MITM apresenta cert servidor falsificado.

**Defesa.** Cliente mTLS verifica cert do servidor contra CA do
banco recebedor. Cert assinado por outra cadeia é rejeitado
(`mtls_rejects_client_from_untrusted_ca`).

### 6.4 Identidade da instituição

**Mecanismo.** Cert do cliente carrega `SAN URI =
urn:mcpix:institution:<id>`. `mtls::extract_institution_id` faz parse
do DER, retorna o ID estruturado. Fallback para CN.

### 6.5 Revogação

**Limite atual.** Não há OCSP/CRL. Rotação de cert comprometido
exige re-emissão pela CA e propagação manual aos peers. Próxima
sessão de PKI completa.

## 7. Superfície de persistência

### 7.1 Vazamento do `SeedStore` local

**Ataque.** Adversário com acesso ao dispositivo lê o store local
(SQLite, KeyChain, EEPROM) e extrai todas as sementes.

**Defesa.** Trait `SeedStore` foi desenhada para que a impl em
produção encapsule Secure Enclave (iOS) / Keystore (Android) /
TPM (PC). Material nunca atravessa userland em prod. A impl in-memory
para demo *não* fornece essa garantia (e está documentado).

### 7.2 Persistência de C₂ em microcontrolador

**Coberto em S11.** `mcpix-embed::storage` (feature `storage`)
implementa `ReceiptStore` e `CounterStore` sobre a abstração
`embedded-storage::NorFlash`. Estratégia ping-pong de 2 slots com
CRC-32 garante:

- **Atomicidade de cold-cut**: queda de energia durante o save
  compromete apenas o slot novo; o anterior continua válido.
- **Detecção de corrupção**: CRC32/ISO-HDLC sobre o record.
- **Anti-rollback de contador**: `CounterStore` persiste `last_t`
  antes da derivação do C₁, eliminando reuso de T entre boots.

Cobertura: 7 testes em `crates/mcpix-embed/src/storage.rs::tests` +
demo bare-metal estendido em `embedded/src/main.rs` que exercita
save → simula reboot → load → valida → mark_consumed.

Backends concretos para os SoCs alvo:
- ESP32 family: [`esp-storage`](https://crates.io/crates/esp-storage)
- STM32: `stm32xx-hal::flash`
- nRF52/53: `nrf-hal::nvmc`
- Testes: `RamFlash` provido pela própria crate.

## 8. Superfície dos códigos públicos

### 8.1 Conteúdo de `C₁` em trânsito

**Análise.** `C₁` é público por design. Não há expectativa de
confidencialidade — viaja em QR code ou texto. A garantia é que
`C₁` *isolado* não revela `S` (HMAC unidirecional) nem permite
fabricar `C₂` válido (requer `S` para o segundo HMAC).

### 8.2 Reuso de `SeedId`

**Análise.** `SeedId` é identificador público estável do recebedor.
Reuso é desejável (mesma chave por dispositivo) e não compromete
segurança porque a derivação inclui contador `T`.

### 8.3 Brute-force de `C₂`

Espaço: `32^11 ≈ 2^55 ≈ 3.6 × 10^16`. Atacante teria que apresentar
candidatos ao recebedor — limitado por throughput do canal de
apresentação (NFC, OCR, manual). Mesmo a 1000 candidatos/s
(otimista), tempo esperado para colisão de aniversário é da ordem de
décadas. Em modo timestamp quantizado, a janela de validade adiciona
outro limite: cada `C₂` vale apenas dentro de seu quantum.

## 9. Resumo de cobertura

| Ataque | Estado | Próximo passo |
|---|---|---|
| Falsificação de instrumento | coberto | — |
| Forjamento de comprovante | coberto | — |
| Replay | coberto | — |
| Timing attack na verificação | coberto | — |
| Colisão de contador | coberto | — |
| Rollback de clock | coberto | — |
| Substituição de binário | coberto | — |
| Patch in-place | coberto | — |
| Comprometimento de CI | coberto (SLSA L3) | L4 hermético |
| LD_PRELOAD / DLL hijack | **não coberto** | remote attestation |
| Eavesdropping inter-bancos | coberto | — |
| Impersonação de banco | coberto | — |
| Cert revogado | **não coberto** | OCSP/CRL |
| Vazamento de seed store | impl-dependente | integração Secure Element |
| Persistência de retained em MCU | coberto (S11) | — |
| Ataque físico ao dispositivo | impl-dependente | Secure Element |
