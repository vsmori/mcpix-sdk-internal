# ADR-0002: Alfabeto restrito do SeedId excluindo `'0'`

## Status

Aceito — implementado em S1, teste de regressão em
`transport_field::tests::seed_id_with_zero_is_rejected`.

## Contexto

O campo de transporte precisa ser parseado por posição em 3 slots
(prefixo, SeedId, C₁) sem campo de tamanho explícito. Para o slot do
SeedId comportar tamanhos variáveis 1..=16, é necessário um
mecanismo de padding que não confunda dados reais com preenchimento.

Considerações:

- Spec original do prompt admite SeedId em `[a-zA-Z0-9]`.
- A faixa de comprimento total `26..=35` é dada por padrão financeiro
  externo; cada char gasto em meta-informação reduz espaço útil.
- Parser embarcado precisa ser O(1) em memória e simples — não cabe
  máquina de estado complexa em ESP8266.

## Decisão

Restringir o alfabeto do `SeedId` a `[a-zA-Z1-9]` (32 chars úteis em
maiúsculas, 26 em minúsculas; total 51) — **excluindo `'0'`**.
Reservar `'0'` como caractere de padding à direita do slot de 16 chars
no campo de transporte.

Implementação:

```rust
// types.rs:78-82
if !value.bytes().all(|b| b.is_ascii_alphanumeric() && b != b'0') {
    return Err(McpixError::SeedIdCharset);
}
```

```rust
// transport_field.rs:21
const SEED_ID_PAD: u8 = b'0';
```

## Alternativas consideradas

### A1. Campo de tamanho explícito (1 byte) antes do SeedId

`PIXOFFv1 ‖ len_char ‖ SeedId(len) ‖ padding(15-len) ‖ C₁(11)`

**Por que não.** Custa 1 chars do orçamento de 35. Adiciona caso de
borda (len inválido). Torna o parser mais complexo. Sem ganho funcional
porque o restritor é igualmente eficaz.

### A2. Separador `':'` entre slots

`PIXOFFv1:SeedId:C₁`

**Por que não.** `':'` não está em `[a-zA-Z0-9]` → quebra a constraint
da faixa externa. Outros separadores alfanuméricos exigiriam
restrição similar à proposta (excluir um char do alfabeto do SeedId).

### A3. Padding com caractere de alfabeto restrito (ex. `'X'`)

**Por que não.** `'X'` é semanticamente plausível em IDs corporativos
("BANK-X-123"). `'0'` é o numérico zero, raramente intencional como
último caractere de identificador comercial (mais frequente é `1`,
`2`, `42`, etc.).

### A4. SeedId fixo de 16 chars (sem padding)

**Por que não.** Força operadores a usarem UUIDs ou IDs aleatórios.
Reduz legibilidade para humanos ("RECVR1" vira "RECVR1XXXXXXXXXX").
Causa fricção operacional desnecessária.

## Consequências

**Positivas:**

- Parser O(1) sem máquina de estado: corta a partir do índice fixo
  e aplica `trim_end_matches('0')`.
- Alfabeto continua coberto por `[a-zA-Z0-9]` da faixa externa.
- Compatibilidade com OCR e digitação manual: `'0'` é menos
  ambíguo no contexto de identificador alfanumérico misto.

**Negativas:**

- `SeedId` legítimo terminado em `'0'` é rejeitado em
  `SeedId::new`. Exemplos rejeitados: "R10", "BANK0", "X12340".
- Documentação obrigatória para integradores: o alfabeto difere
  ligeiramente da expectativa `[a-zA-Z0-9]` plana.

## Validação

| Caso | Comportamento esperado | Teste |
|---|---|---|
| `"R12"` → encode → parse | round-trip preserva `"R12"` | `seed_id_with_internal_digits_roundtrips` |
| `"R10"` em `SeedId::new` | retorna `Err(SeedIdCharset)` | `seed_id_with_zero_is_rejected` |
| `"R01"` em `SeedId::new` | retorna `Err(SeedIdCharset)` | mesmo teste |
| Propriedade geral | round-trip vale para todo SeedId válido | `properties::encode_parse_roundtrip` |

## Referências

- RFC 4648 §6 (Base 32) — discussão de alfabetos sem ambiguidade
  visual.
- Wikipedia, "Base32" — Crockford's alphabet (motivação por exclusão
  de `I`, `L`, `O`, `U`).
