# ADR-0001: HMAC-SHA-256 com domain separation para C₁ e C₂

## Status

Aceito — implementado em S1, propriedades validadas em S5.

## Contexto

O protocolo exige duas derivações distintas a partir da mesma semente
`S` e do mesmo contador `T`: um código público (`C₁`) e um código
retido localmente (`C₂`). Sem cuidado, é possível que um atacante
manipule o protocolo para fazer uma chamada que se passe por outra,
ou que confunda os papéis das duas saídas.

Considerações adicionais:

- `C₂` precisa depender explicitamente de `C₁`, não apenas de `(S, T)`
  — propriedade exigida pela definição de "par atômico encadeado".
- A escolha de algoritmo precisa ser portável para microcontroladores
  (ESP8266, Cortex-M).
- Resistência mínima esperada: `2^128` (segurança de SHA-256 truncado).

## Decisão

Adotar HMAC-SHA-256 como única primitiva de derivação, com tags de
**domain separation** distintas para `C₁` e `C₂`:

```
dom_c1 = "mcpix/v1/c1"        (11 bytes ASCII)
dom_c2 = "mcpix/v1/c2"        (11 bytes ASCII)

C₁ = trunc(HMAC(S, dom_c1 ‖ T_be), 11 chars)
C₂ = trunc(HMAC(S, dom_c2 ‖ T_be ‖ ASCII(C₁)), 11 chars)
```

Truncamento converte os 256 bits de saída em 11 caracteres base32 custom
(55 bits efetivos).

## Alternativas consideradas

### A1. Duas chaves HKDF derivadas de `S`

Usar HKDF para derivar `K_c1` e `K_c2` a partir de `S`, então `C_i =
HMAC(K_ci, T)`.

**Por que não.** Adiciona uma camada (HKDF + HMAC = duas operações) sem
ganho de segurança em relação a domain separation simples. Aumenta o
footprint embarcado em ~600 bytes de código.

### A2. SHA-256 direto sem HMAC

`C_i = trunc(SHA256(S ‖ dom ‖ T))`.

**Por que não.** SHA-256 cru com chave concatenada é vulnerável a
length-extension. HMAC tem o padding interno que neutraliza isso. A
diferença de custo entre HMAC-SHA-256 e SHA-256 cru é de ~200ns no
host e ~5μs em Cortex-M0+ — irrelevante para o caso de uso.

### A3. BLAKE3 ou outras hashes modernas

**Por que não.** BLAKE3 é mais rápido mas tem suporte menor em
ambientes restritos. SHA-256 está em literalmente todo Secure Element
comercial. Optar por SHA-256 maximiza portabilidade.

### A4. Encadeamento implícito via cadeia de derivação

`K_c2 = HMAC(S, C₁); C₂ = trunc(K_c2)`.

**Por que não.** Equivalente em segurança ao que adotamos, mas requer
o caller passar `C₁` como chave para a segunda chamada — quebra o
modelo "HMAC com `S` como chave". Mantemos `S` como chave única para
simplicidade de revisão.

## Consequências

**Positivas:**

- Determinismo absoluto: implementações independentes (Rust host,
  Rust no_std, Swift, Kotlin, .NET) convergem ao mesmo `(C₁, C₂)`.
  Validado em `crates/mcpix-embed/tests/cross_validate.rs`.
- Footprint mínimo: HMAC-SHA-256 é ~3KB de código em Cortex-M;
  cabe em qualquer MCU alvo.
- Auditabilidade: o algoritmo cabe em uma página de papel; revisor
  pode verificar manualmente a derivação de um teste.

**Negativas:**

- Truncamento de 256 bits para 55 bits (11 chars × 5 bits) reduz
  espaço de busca cega para `2^55 ≈ 3.6 × 10^16`. Mitigado por
  combinação com defesa de replay e janela de timestamp — `C₂` válido
  vale apenas dentro de seu quantum.

## Validação

| Propriedade | Teste |
|---|---|
| Determinismo `(S,T) → (C₁, C₂)` | `crypto::tests::derive_pair_is_deterministic` |
| `C₂` recuperável de `C₁` | `crypto::tests::c2_derives_from_c1_consistently` |
| Counters diferentes → pares diferentes | `crypto::tests::different_counter_yields_different_pair` |
| Seeds diferentes → pares diferentes | `crypto::tests::different_seed_yields_different_pair` |
| Encadeamento universal | `properties::c2_recoverable_from_c1` (proptest, 256+ casos) |

## Referências

- Krawczyk, Bellare, Canetti. "HMAC: Keyed-Hashing for Message
  Authentication" (RFC 2104).
- NIST FIPS 198-1 — The Keyed-Hash Message Authentication Code.
- NIST SP 800-108 §4 — Key Derivation Functions Using Keyed-Hash
  Functions (justifica domain separation por label).
