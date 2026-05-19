# Glossário

Termos do protocolo em uso ao longo da documentação. Mantido curto e
não-circular: cada definição faz referência apenas a termos definidos
anteriormente.

## A

**Alfabeto base32 custom.** Conjunto de 32 caracteres ASCII
alfanuméricos `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`. Exclui `I`, `L`, `O`,
`0`, `1` para evitar ambiguidade visual em OCR e digitação. Usado
para codificar `C₁` e `C₂`.

## B

**Banco do pagador.** Instituição que processa o pagamento iniciado
pelo pagador. Atua como **substituto institucional** do recebedor na
recomposição de `C₂`.

**Banco do recebedor.** Instituição que custodia a semente `S` do
recebedor. Atende lookup autenticado por mTLS.

## C

**`C₁`** (código de cobrança). Primeiro elemento do par atômico,
11 caracteres no alfabeto base32 custom. Público; embarcado no campo
de transporte.

**`C₂`** (código de confirmação). Segundo elemento do par atômico,
11 caracteres no mesmo alfabeto. Material sensível: retido localmente
pelo recebedor até o momento da validação.

**Campo de transporte.** String pública alfanumérica de 35 chars que
veicula o instrumento de cobrança. Layout: `PIXOFFv1‖SeedId(16)‖C₁(11)`.

**CounterCollision.** Erro retornado pelo `TimestampQuantizedCounter`
quando uma segunda cobrança é solicitada para o mesmo `SeedId` dentro
do mesmo quantum de tempo.

**CounterRollback.** Erro retornado pelo `TimestampQuantizedCounter`
quando o relógio do sistema recua abaixo do último `T` emitido para
o `SeedId`.

## D

**Domain separation.** Técnica que injeta tag única em cada papel de
uma derivação para impedir confusão entre saídas. Aqui:
`dom_c1 = "mcpix/v1/c1"`, `dom_c2 = "mcpix/v1/c2"`.

## E

**Ed25519.** Esquema de assinatura digital baseado em curva elíptica
Edwards de 25519 bits. Usado para assinar `SHA256SUMS` do release.
Chave pública 32 bytes, assinatura 64 bytes.

## F

**FFI (Foreign Function Interface).** Camada que expõe funções Rust
para outras linguagens via C-ABI. Aqui: `mcpix-ffi` para .NET P/Invoke
e `mcpix-uniffi` para Swift / Kotlin via UniFFI.

## H

**HMAC-SHA-256.** Função de autenticação de mensagem (Hash-based
Message Authentication Code) baseada em SHA-256. Saída de 32 bytes
(256 bits). Função criptográfica unidirecional usada para derivar
`(C₁, C₂)` a partir de `(S, T)`.

## I

**Instrumento de cobrança.** Synonym para campo de transporte quando
considerado do ponto de vista do recebedor (que o emite).

## M

**mTLS** (Mutual TLS). Protocolo de transporte autenticado em ambas
direções: cliente e servidor apresentam certificados verificados
mutuamente contra CA federada.

**McpixError.** Enum único de erro retornado por todas as funções
públicas da SDK. Variants mapeados 1:1 para códigos numéricos na C-ABI.

## P

**Pagador.** Ator que recebe o instrumento de cobrança e o submete ao
banco do pagador. Não detém chaves; intermedia.

**Par atômico.** O conjunto `(C₁, C₂)` derivado de `(S, T)`. Atômico
no sentido de inseparável: `C₂` depende de `C₁` por encadeamento.

**PIXOFFv1.** Prefixo constante de 8 caracteres que identifica a
versão do esquema no campo de transporte. Sinaliza ao banco do
pagador a presença do mecanismo.

## Q

**Quantum.** Em modo timestamp quantizado, intervalo de tempo de
`window_seconds` (padrão 30s) dentro do qual o valor de `T` é
constante. `T = ⌊now_unix_secs / window_seconds⌋`.

## R

**Recebedor.** Ator que opera offline e gera o par `(C₁, C₂)`. Detém
a semente `S` localmente.

**RELEASE_PUBKEY.** Chave pública Ed25519 (32 bytes raw) embarcada no
binário do SDK em compile time via `include_bytes!`. Usada para
verificar `SHA256SUMS.sig` no `verify_self`.

**Retained receipt** (`RetainedReceipt`). Registro local mantido pelo
recebedor após `generate_charge`, contendo `(SeedId, T, amount,
expected_C₂, consumed)`. Consultado no momento da validação.

## S

**Substituição institucional.** Propriedade do protocolo: o banco do
pagador reconstrói `C₂` aplicando a mesma derivação que o recebedor
aplicou offline, sem qualquer canal direto entre eles. Viabilizada
pelo determinismo da derivação e pela consulta autenticada de `S` no
banco do recebedor.

**`S`** (semente). Material criptográfico de 256 bits compartilhado
entre recebedor e banco do recebedor. Chave para todas as derivações
HMAC.

**SAN URI.** Extensão X.509 Subject Alternative Name no formato URI.
Usada para carregar `urn:mcpix:institution:<id>` no certificado do
cliente mTLS, identificando a instituição.

**SeedId.** Identificador público do recebedor, 1..=16 chars no
alfabeto `[a-zA-Z1-9]`. Excluído `'0'` (reservado como caractere de
padding no slot do campo de transporte).

**SeedStore.** Trait que abstrai a persistência local do recebedor.
Implementações: in-memory (demo), SQLite (`sqlite` feature),
Secure Element / HSM (futuro).

**SHA256SUMS.** Arquivo texto contendo `<hash> <path>` para todo
artefato do release. Assinado por chave Ed25519 privada → produz
`SHA256SUMS.sig`.

## T

**`T`** (contador). Parâmetro variável da derivação. Dois modos:
sequencial (`InMemoryCounter`) ou timestamp quantizado
(`TimestampQuantizedCounter`).

**Tempo constante** (comparação). Operação de comparação cuja
duração não depende dos valores comparados. Implementada via
`subtle::ConstantTimeEq`.

## U

**UniFFI.** Ferramenta da Mozilla que gera bindings Swift e Kotlin
a partir de annotations `#[uniffi::export]` em código Rust. Usada
para distribuir o SDK em iOS e Android.

## V

**ValidationOutcome.** Enum retornado por `validate_receipt`. Valores:
`Valid`, `Mismatch`, `Replay`.

**verify_self.** Função do `mcpix-receiver-sdk` que lê o binário
carregado, encontra `SHA256SUMS` + `SHA256SUMS.sig` adjacentes,
valida assinatura com `RELEASE_PUBKEY` e compara o hash do binário
contra o manifesto. Retorna `Verified`, `Tampered` ou `Skipped`.

## W

**Window seconds.** Tamanho do quantum no modo timestamp quantizado.
Default 30 (RFC 6238-style). Configurável.
