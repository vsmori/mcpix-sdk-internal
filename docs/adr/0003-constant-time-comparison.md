# ADR-0003: Comparação em tempo constante via `subtle`

## Status

Aceito — implementado em S1, propriedade adicionada em S5.

## Contexto

`validate_receipt` compara o `C₂` retido localmente pelo recebedor
com o `C₂` apresentado por canal arbitrário (OCR, NFC, digitação).
Um operador `==` ingênuo em strings/arrays Rust termina no primeiro
byte divergente, o que abre canal de timing side-channel.

Cenário concreto:

1. Atacante apresenta `C₂` arbitrário ao dispositivo do recebedor.
2. Mede o tempo entre apresentação e BIP/timeout via canal observável
   (LED, beep, latência de comunicação NFC).
3. Para cada um dos 11 bytes do `C₂`, varre os 32 valores possíveis
   do alfabeto e observa qual produz tempo maior — esse é o byte
   correto.
4. Custo total: `O(11 × 32) = 352` apresentações em média.

Sem mitigação, a busca cega "2^55 chamadas" colapsa para "~350 chamadas",
viável em campo via NFC continuamente.

## Decisão

Adotar a crate [`subtle`](https://crates.io/crates/subtle) (mantida
pelo time da Dalek Cryptography) e usar `ConstantTimeEq` para toda
comparação de material criptográfico:

```rust
// crypto.rs:121
pub fn verify_c2(expected: &C2, presented: &C2) -> bool {
    expected.0.ct_eq(&presented.0).into()
}
```

`subtle::ConstantTimeEq` aplica XOR byte-a-byte sobre toda a sequência,
acumula em volátil de tamanho fixo e converte ao final — sequência de
instruções é independente do conteúdo.

## Alternativas consideradas

### A1. Implementação inline com XOR acumulado

```rust
let mut diff = 0u8;
for (a, b) in expected.iter().zip(presented.iter()) {
    diff |= a ^ b;
}
diff == 0
```

**Por que não.** Funciona em compilação O0 mas o LLVM pode otimizar
para early-exit em opt-level=3, anulando a defesa. `subtle` usa
black_box e barreiras explícitas para impedir essa otimização — é
o trabalho que não queremos reimplementar.

### A2. `==` ingênuo + adicionar delay aleatório

**Por que não.** Atacante apenas precisa de muitas amostras para
filtrar o ruído. Defesa baseada em jitter é estatisticamente
quebrável. Tempo constante real é categoricamente diferente.

### A3. `ring::constant_time::verify_slices_are_equal`

**Por que não.** `ring` é dep grande (~500 KB) e não compila em
`no_std` simples. `subtle` é ~5 KB, `no_std` first-class, e é
exatamente para esse caso de uso.

## Consequências

**Positivas:**

- Defesa de timing aplicada uniformemente em `mcpix-core` e
  `mcpix-embed`. Mesma crate, mesma garantia.
- Custo runtime negligível: ~50ns para 11 bytes em x86_64; ~500ns
  em Cortex-M0.
- Reduzimos área de cripto-engenharia manual: usamos primitiva
  auditada.

**Negativas:**

- Dependência externa adicional (mas é a única forma honesta de
  garantir a propriedade em Rust). `subtle` tem 0 deps próprias.

## Validação

| Propriedade | Teste |
|---|---|
| `verify_c2(a, b) == (a == b)` para qualquer `(a, b)` | `crypto::tests::verify_accepts_match`, `verify_rejects_mismatch` |
| Equivalência universal | `properties::verify_c2_equiv_to_equality` (proptest) |

A propriedade *timing* per se não é diretamente testada — confiamos
na implementação de `subtle` e seus testes próprios. Testes
diferenciais de timing em CI exigiriam infra dedicada (medições
estatísticas com Welch t-test sobre milhões de samples) e ficam
fora do escopo desta entrega.

## Referências

- Kocher, "Timing Attacks on Implementations of Diffie-Hellman, RSA,
  DSS, and Other Systems" (CRYPTO '96) — paper seminal do ataque.
- Bernstein, "Cache-timing attacks on AES" (2005) — implicações
  arquiteturais.
- [`subtle`](https://docs.rs/subtle) — documentação da crate
  adotada, incluindo justificativa das barreiras de otimização.
