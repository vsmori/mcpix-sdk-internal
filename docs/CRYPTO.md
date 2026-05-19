# Especificação criptográfica formal

Este documento descreve precisamente as operações criptográficas que
sustentam o protocolo. É o material primário para revisão criptográfica
externa e para o exame técnico do pedido PCT.

## 1. Notação

| Símbolo | Significado | Tamanho |
|---|---|---|
| `S` | semente compartilhada recebedor↔banco-recebedor | 256 bits |
| `T` | contador unidirecional (sequencial ou `⌊now/window⌋`) | 64 bits big-endian |
| `C₁` | código de cobrança (público) | 11 chars alfanuméricos |
| `C₂` | código de confirmação (retido localmente) | 11 chars alfanuméricos |
| `H(K, m)` | HMAC-SHA-256 com chave `K` e mensagem `m` | 256 bits |
| `‖` | concatenação de bytes | |
| `enc₃₂(b, n)` | codificação base32 custom dos primeiros `n` chars (5 bits cada) | |

## 2. Derivação do par atômico

Dado `(S, T)`, computa-se o par `(C₁, C₂)` em duas chamadas HMAC:

```
C₁ = enc₃₂( H(S, dom_c1 ‖ T_be), 11 )
C₂ = enc₃₂( H(S, dom_c2 ‖ T_be ‖ ASCII(C₁)), 11 )
```

onde:

- `dom_c1 = "mcpix/v1/c1"` (11 bytes ASCII)
- `dom_c2 = "mcpix/v1/c2"` (11 bytes ASCII)
- `T_be` = `T` em representação big-endian, 8 bytes
- `ASCII(C₁)` = os 11 bytes da string codificada (não os bits brutos)

### 2.1 Por que dois tags de domínio?

Sem domain separation, um atacante poderia consultar uma chamada
no papel de "C₁" e reusar a saída como "C₂" para outra `T`. Os tags
`dom_c1` e `dom_c2` injetam dependência funcional no papel da
derivação, garantindo `H(S, dom_c1 ‖ T) ≠ H(S, dom_c2 ‖ T)` por
construção (HMAC é PRF; entradas distintas produzem saídas
independentes para o adversário).

Referência: NIST SP 800-108 §4 (key derivation com etiqueta).

### 2.2 Encadeamento C₁ → C₂

`C₂` não é derivado apenas de `(S, T)` — usa também `C₁` como
material de entrada. Consequência prática:

> Para conhecer `C₂` é necessário conhecer `C₁`. Não há atalho a
> partir de `(S, T)` sem passar pela computação de `C₁` primeiro.

Isso fortalece a noção de "par atômico": os dois códigos estão
ligados pelo encadeamento, não são derivações independentes do mesmo
segredo.

Detalhamento em [adr/0001-domain-separated-hmac.md](./adr/0001-domain-separated-hmac.md).

## 3. Codificação base32 custom

Alfabeto:

```
ABCDEFGHJKLMNPQRSTUVWXYZ23456789
```

32 caracteres — exatamente 5 bits por símbolo. Excluídos
deliberadamente: `I`, `L`, `O`, `0`, `1` (ambiguidade visual em
OCR/digitação manual).

Para `enc₃₂(bytes, n)`:

1. Acumular bits dos bytes de entrada em buffer FIFO.
2. Para cada caractere de saída: extrair top 5 bits, indexar alfabeto.
3. Repetir `n` vezes. Bits residuais do hash de 256 bits são
   descartados após produzir `n` caracteres (consumimos `⌈5n/8⌉ = ⌈55/8⌉ = 7` bytes).

Implementação: `crates/mcpix-core/src/crypto.rs:25`.

### 3.1 Distribuição de saída

Como HMAC-SHA-256 é uma PRF cuja saída é indistinguível de uniforme,
e o encoder consome bits em ordem MSB-first sem viés, os 11 caracteres
de `C₁` e `C₂` são uniformemente distribuídos sobre o alfabeto de 32
símbolos. Espaço de busca por brute-force: `32^11 = 2^55 ≈ 3.6 × 10^16`.

## 4. Codificação do campo de transporte

Layout posicional de **35 caracteres alfanuméricos**:

```
┌────────────┬─────────────────────┬──────────────┐
│ PIXOFFv1   │ SeedId (16, pad '0')│ C₁ (11)      │
│ 8 chars    │ 16 chars            │ 11 chars     │
└────────────┴─────────────────────┴──────────────┘
```

- **Prefixo** (`PIXOFFv1`): constante de versão. Triagem em O(1) pelo
  banco do pagador.
- **SeedId**: identificador público do recebedor. Alfabeto restrito a
  `[a-zA-Z1-9]` (note exclusão de `0`); `'0'` é reservado como
  caractere de padding à direita. Tamanho efetivo 1..=16.
- **C₁**: alfabeto `ABCDEFGHJKLMNPQRSTUVWXYZ23456789` (32 chars).

### 4.1 Por que `'0'` é reservado para padding

Para fazer parsing posicional simples sem campo de tamanho explícito,
o slot de 16 chars do `SeedId` é pad-right com `'0'`. Como `'0'` está
excluído do alfabeto do `SeedId`, o parser pode aplicar
`trim_end_matches('0')` sem ambiguidade. Detalhamento em
[adr/0002-restricted-seed-id-alphabet.md](./adr/0002-restricted-seed-id-alphabet.md).

### 4.2 Compatibilidade com formato externo

A faixa `[a-zA-Z0-9]{26,35}` é a constraint do campo de transporte
adotada por padrão financeiro brasileiro. O layout fixo de 35 chars
sente-se dentro da janela superior. Versões futuras podem usar
comprimentos diferentes desde que continuem dentro da janela 26-35.

## 5. Comparação em tempo constante

`verify_c2(expected, presented)` invoca `subtle::ConstantTimeEq` —
implementação em Rust safe que executa em tempo independente do
conteúdo (não termina early no primeiro byte divergente).

### 5.1 Por que isso importa

Atacante com controle sobre `presented_C₂` e capacidade de medir
latência do `validate_receipt` (ex. injetando o comprovante via NFC
e observando o BIP/timeout) pode, com `==` ingênuo, recuperar o
`expected_C₂` byte-a-byte:

1. Variar primeiro byte → encontra valor que demora 1 ciclo extra.
2. Fixar primeiro byte, variar segundo → encontra próximo byte.
3. Repetir para 11 bytes.

Custo: `O(11 × 32) = 352` consultas em média, viável em campo. Defesa
em tempo constante elimina o sinal.

Referência: ver `crypto.rs:121` para o comentário inline justificando
o uso de `subtle`.

## 6. Geração de semente

`receiver_sdk::register`:

1. Aloca `[u8; 32]` na stack.
2. Preenche via `OsRng::try_fill_bytes` (`getrandom` do OS).
3. Constrói `Seed::from_bytes`.
4. Encaminha para `SeedStore::put_seed`.

A `Seed` implementa `ZeroizeOnDrop` (crate `zeroize` com `derive`);
ao sair de escopo, o material é sobrescrito com zeros pela função
zeroize-aware, com garantia de que o compilador não otimiza a
sobrescrita.

Em produção, a geração migra para Secure Enclave/HSM — a interface
`SeedStore` foi desenhada para essa substituição sem mudar o núcleo.

## 7. Verificação de integridade do binário

Duas camadas:

### 7.1 SHA-256 self-check (S3)

`MCPIX_EXPECTED_SHA256` é o hash do binário esperado, carimbado em
build time via `option_env!`. Em runtime, `verify_self()` lê o
próprio `.so`/`.dylib`/`.dll` do disco e compara com o hash carimbado.

### 7.2 Manifesto assinado (S4)

`SHA256SUMS` (lista de hashes para todos os artefatos do release)
recebe assinatura Ed25519 (`SHA256SUMS.sig`) com chave de release.
A chave **pública** correspondente está em
`crates/mcpix-core/trusted_keys/release.pub` (32 bytes raw),
embarcada no binário via `include_bytes!`.

Verificação combinada em runtime:

1. Carrega `SHA256SUMS` + `SHA256SUMS.sig` adjacentes ao binário.
2. Verifica assinatura com `RELEASE_PUBKEY`.
3. Localiza linha de `SHA256SUMS` correspondente ao basename do
   binário carregado.
4. Recomputa SHA-256 do binário e compara.

Falha em qualquer passo → `IntegrityCheck::Tampered` ou
`InvalidSignature`.

## 8. Vetores de teste

Conjunto curto cobrindo casos limítrofes; cross-validado entre
`mcpix-core` (host std) e `mcpix-embed` (no_std). Ver
`crates/mcpix-embed/tests/cross_validate.rs`.

| `seed[0]` repete 32x | `T` | `C₁` (esperado) | `C₂` (esperado) |
|---|---|---|---|
| `0x00` | `0` | (gerado) | (gerado) |
| `0xAB` | `1` | (gerado) | (gerado) |
| `0x42` | `42` | (gerado) | (gerado) |
| `0xFF` | `u64::MAX` | (gerado) | (gerado) |
| `0x77` | `56666667` | (gerado) | (gerado) |

Valores explícitos não são tabulados aqui porque cross-validate gera
e compara em runtime — qualquer drift entre `core` e `embed` quebra
o teste imediatamente. Para gerar a tabela: rodar
`cargo test -p mcpix-embed cross_validate -- --nocapture`.

## 9. Garantias de segurança alegadas

| Propriedade | Mecanismo | Confiança |
|---|---|---|
| Determinismo `(S,T) → (C₁,C₂)` | HMAC-SHA-256 é função | matemática |
| Segredo de `S` é necessário para reproduzir `C₂` | HMAC com `S` como chave; sem `S`, atacante reduz a busca cega `2^256` | criptográfica |
| `C₂` não derivável de `C₁` sem `S` | encadeamento `C₂ = H(S, ... ‖ C₁)` exige `S` | criptográfica |
| Cobrança não pode ser reusada | `consumed` flag + replay defense no store | engenharia |
| Resistência a timing attacks no `verify` | `subtle::ConstantTimeEq` | implementação |
| Substituição institucional bit-exata | determinismo + dom-sep + encadeamento | matemática |

Limites:

- **Não cobre** comprometimento da semente `S`. Se atacante obtém `S`,
  produz `C₂` para qualquer `T` à vontade. Defesa: custódia de `S` em
  Secure Element / HSM, fora do escopo deste documento.
- **Não cobre** ataque físico ao dispositivo do recebedor (extração de
  flash, leitura via JTAG). Defesa: ver
  [THREAT_MODEL.md §6](./THREAT_MODEL.md#6-ataques-físicos).
