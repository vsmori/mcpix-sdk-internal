# ADR-0007: Suporte dual a `T` sequencial e timestamp quantizado

## Status

Aceito — implementado em S6. Implementações em
`crates/mcpix-receiver-sdk/src/{monotonic_counter,timestamp_counter}.rs`.

## Contexto

A reivindicação técnica admite o parâmetro variável `T` como
**contador unidirecional** ou **timestamp quantizado**. Cada modo
endereça cenários operacionais distintos:

- **Sequencial** assume estado persistido no recebedor (flash/HSM).
  Adequado a dispositivos sem clock confiável (MCU sem RTC, dispositivos
  air-gapped longos). Não requer sincronização externa.
- **Timestamp quantizado** (estilo RFC 6238 TOTP) deriva `T` do clock
  de parede. Recebedor e banco do pagador convergem ao mesmo `T` se
  seus clocks estão dentro da janela de tolerância. Adequado a
  cenários online ou semi-online onde clocks NTP estão razoavelmente
  sincronizados.

A trait `Counter::next(&self, &SeedId) -> Result<u64, McpixError>` é
expressiva o suficiente para ambos.

## Decisão

Fornecer **duas implementações concretas** da trait `Counter`, ambas
parte do `mcpix-receiver-sdk`. Operador escolhe na construção do
SDK qual injetar.

### `InMemoryCounter` (sequencial)

- Estado: `HashMap<SeedId, u64>`, valor inicial 0.
- `next()`: incrementa em 1, retorna novo valor. Overflow → `CounterOverflow`.
- Em produção: substituir por impl que persiste em flash/HSM antes de
  cada retorno.

### `TimestampQuantizedCounter`

- Estado: `HashMap<SeedId, u64>` com `last_issued`.
- `next()`: computa `T = clock.now_unix_secs() / window_seconds`.
  - Se `T > last_issued`: avança, retorna `T`.
  - Se `T == last_issued`: `CounterCollision { window_seconds }`.
  - Se `T < last_issued`: `CounterRollback { last, now: T }`.
- `current_quantum()` (sem mutar estado) para o banco do pagador
  derivar `T` esperado a partir do próprio clock.
- Window default: 30s (RFC 6238). Configurável via `with_window`.

### Tolerância de drift entre lados

O banco do pagador, recebendo o instrumento, não conhece o `T` exato
do recebedor — sabe apenas o seu próprio quantum. Para tolerar drift
de até `N` janelas:

- `process_payment_windowed` produz `2N+1` candidatos `C₂` para
  `T ∈ [T_now - N, T_now + N]`.
- Recebedor tenta cada candidato; primeiro match no retained vence.
- Default `N = 1` → tolera ±30s de drift.

## Alternativas consideradas

### A1. Apenas sequencial

**Por que não.** Cobre menos do que a reivindicação admite. Exige
persistência confiável do contador entre boots — comum em servidor,
problemático em dispositivos pequenos.

### A2. Apenas timestamp quantizado

**Por que não.** Exige clock razoavelmente sincronizado em ambos os
lados. Dispositivos sem RTC ou com clock drifting (ESP8266 sem NTP
recente) falhariam. Sequencial é o fallback robusto.

### A3. Modo híbrido `T = quantum × MAX_INNER + intra_seq`

Permite múltiplas cobranças no mesmo quantum.

**Por que não.** Complica a comunicação banco↔recebedor (intra_seq
teria que viajar no instrumento) e dobra o espaço de busca do
atacante por quantum. Padrão TOTP rejeita também — "uma chamada
por quantum, falhe se repetir" é mais simples e auditável.

### A4. Counter persistido em store

Adicionar `last_counter` ao `SeedStore`.

**Por que não.** Acopla persistência do counter à persistência do
store, complicando substituição futura por HSM (onde counter pode
viver em monotonic counter dedicado do hardware). Mantemos counter
como trait separada.

## Consequências

**Positivas:**

- Operador escolhe modo apropriado por device class via injeção de
  trait. Sem `cfg` no núcleo.
- Defesa explícita contra ataques de relógio (rollback) — não silencia.
- Drift tolerance é política do banco, não do protocolo.

**Negativas:**

- Recebedor com modo timestamp tem limite de **1 cobrança por
  quantum por SeedId**. Cenário operacional excepcional (vending
  machine processando muitas transações por minuto) exige `window`
  menor ou modo sequencial.

## Validação

| Cenário | Teste |
|---|---|
| Modo sequencial determinístico | `monotonic_counter` (testes inline) |
| Timestamp quantizado básico | `timestamp_counter::tests::first_call_uses_quantized_now` |
| Colisão na mesma janela | `timestamp_counter::tests::same_window_call_is_rejected` |
| Cross-window normal | `timestamp_counter::tests::cross_window_call_succeeds` |
| Rollback rejeitado | `timestamp_counter::tests::clock_rollback_is_rejected` |
| Isolamento entre SeedIds | `timestamp_counter::tests::isolation_between_seed_ids` |
| Tolerância windowed | `mcpix-bank-payer-mock::tests::windowed_produces_2n_plus_1_candidates` |
| Drift tolerável | `windowed_with_drifted_clock_still_includes_correct_quantum` |
| Drift além de tolerância | `windowed_excludes_drift_beyond_tolerance` |

## Referências

- RFC 6238 — TOTP: Time-Based One-Time Password Algorithm.
- NIST SP 800-63B §5.1.4 — Out-of-Band Devices (discussão de
  janelas e tolerância).
