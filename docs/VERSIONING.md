# Versionamento do protocolo

Este documento define **o que** é versionado, **como** novas versões
são introduzidas, e **quanto tempo** versões antigas continuam
parseáveis pelos peers.

> A versão da SDK (semver do crate, `0.1.x`) é independente da versão
> do **protocolo** (`PIXOFFv1`, `PIXOFFv2`, …). Uma SDK pode emitir e
> parsear múltiplas versões ao mesmo tempo.

## O que está sob versão

Uma `ProtocolVersion` (ver `mcpix_core::version`) congela:

| Aspecto | V1 |
|---|---|
| Prefixo wire | `PIXOFFv1` (8 bytes ASCII) |
| Comprimento total do campo | 35 chars |
| Layout posicional | prefix 8 · SeedId-slot 16 · C₁ 11 |
| Alfabeto SeedId | `[a-zA-Z1-9]` (sem `0`, reservado para pad) |
| Alfabeto C₁/C₂ | base32 Crockford-like (`ABCDEFGHJKLMNPQRSTUVWXYZ23456789`) |
| KDF | HMAC-SHA256 |
| Tamanho da Seed | 32 bytes |

Mudar qualquer linha desta tabela exige nova `ProtocolVersion`.

## O que NÃO está sob versão

- API Rust/Swift/Kotlin/.NET — esta segue semver da SDK, sob outras
  regras.
- Persistência local (`SeedStore`, `Counter`) — formato de storage do
  app, opaco ao protocolo.
- Modelo de transporte inter-bancos (mTLS, CRL, OCSP) — política
  operacional, ortogonal.

## Detecção e dispatch (atual: V1)

```text
   wire bytes
       │
       ▼
   version::detect()  ────►  PIXOFFv1                ┐
       │                       │                     │
       │                       └─ transport_field::parse_v1()
       │
       ├──────────►  PIXOFFvN (N desconhecido)
       │                ↓
       │            McpixError::UnsupportedProtocolVersion("PIXOFFvN")
       │                ↓
       │            UX: "atualize sua SDK"
       │
       └──────────►  prefixo fora da família PIXOFFv*
                        ↓
                    McpixError::TransportFieldPrefix
                        ↓
                    UX: "essa string não é nosso protocolo"
```

**Por que separar `UnsupportedProtocolVersion` de `TransportFieldPrefix`:**
o usuário com SDK antigo que escaneia uma cobrança v2 precisa saber
*o que* atualizar — "atualize seu app de banco" — e não confundir com
"isto não é uma cobrança PIX".

## Capability negotiation inter-bancos

Antes de iniciar uma transação, o banco do pagador pode (e deve)
consultar quais versões o banco do recebedor suporta:

```
GET /v1/capabilities
HTTP/1.1 200 OK
Content-Type: application/json

{ "versions": ["PIXOFFv1"] }
```

A SDK provê tanto o endpoint (`mcpix_bank_receiver::http_server`)
quanto o cliente (`HttpBankReceiver::supported_versions`) e o helper
de seleção (`mcpix_core::version::negotiate_version`):

```rust
let client = HttpBankReceiver::new(base_url);
let peer = client.supported_versions()?;
let peer_strs: Vec<String> = peer.iter().map(|v| v.prefix().to_string()).collect();

let agreed = version::negotiate_version(ProtocolVersion::all(), &peer_strs)
    .ok_or_else(|| /* sem versão comum: abortar transação */)?;
```

Política embutida:

- **Maior comum primeiro**: `negotiate_version` itera `local.iter().rev()` e
  escolhe a maior versão presente em ambos. Para introduzir V2 sem
  flag-day, basta os dois lados subirem a SDK; bancos legados continuam
  vendo V1 mutuamente.
- **Strings, não enums, no fio**: a resposta é `["PIXOFFv1"]`, não
  inteiros. Um peer que anuncie `PIXOFFv2` para um cliente V1-only
  passa pelo JSON sem quebrar — só é filtrado na negociação.
- **`HttpBankReceiver::supported_versions`** filtra para o que **este
  build** conhece; para preservar a info crua (e.g. relatar "peer
  fala v9 mas não sabemos") use `negotiate_version` diretamente sobre
  o JSON.

A infraestrutura **não decide** o que fazer com `None` — caller
decide:

| Caso | Ação típica |
|---|---|
| `Some(V_n)` | Prosseguir; emitir cobranças em V_n |
| `None` | Abortar transação; logar "no common version" para alertar operações |

## Política de introdução de nova versão

### Quando bumper

Bump de versão é **breaking change** do wire format. Triggers:

1. Mudança de algoritmo criptográfico (HMAC → BLAKE3, etc.).
2. Mudança de alfabeto ou comprimento de C₁/C₂.
3. Mudança de layout posicional do campo de transporte.
4. Adição de campo obrigatório (não-skippable) no campo público.

Adições opcionais (extensões TLV, hints de issuer) **não** bumpam
versão — desde que a versão antiga continue conseguindo parsear o
campo ignorando o extra.

### Como bumper (passo a passo)

1. **Adicionar variante** em `mcpix_core::version::ProtocolVersion`:
   ```rust
   pub enum ProtocolVersion {
       V1 = 1,
       V2 = 2,   // novo
   }
   ```
   Discriminantes existentes são imutáveis — invariante ABI.

2. **Registrar prefixo** em `prefix()` e em `all()`:
   ```rust
   Self::V2 => "PIXOFFv2",
   ```
   ```rust
   pub const fn all() -> &'static [Self] { &[Self::V1, Self::V2] }
   ```

3. **Implementar parser v2** num arquivo paralelo
   `transport_field_v2.rs` exportando `parse_v2(field) -> ParsedField`.
   Não tocar em `parse_v1`.

4. **Estender o dispatch** em `transport_field::parse`:
   ```rust
   match version::detect(field)? {
       ProtocolVersion::V1 => parse_v1(field),
       ProtocolVersion::V2 => parse_v2(field),
   }
   ```

5. **Decidir a versão default emitida** — `ProtocolVersion::current()`.
   Default permanece V1 até que ≥80% dos peers conhecidos suportem V2
   (política operacional, validar via telemetria do
   `BankReceiver::lookup_seed`). Caller que quer emitir já em V2 usa
   `transport_field::encode_with_version(_, _, V2)`.

6. **Atualizar bindings**: adicionar novos códigos de erro (se houver)
   em `mcpix-ffi/src/error.rs` (números **novos**, nunca reusar) e
   regenerar `bindings/{c,dotnet,swift,kotlin}` via
   `cargo xtask gen-bindings`.

7. **Documentar** em `CHANGELOG.md` o que muda no wire e qual a
   janela de coexistência V1↔V2.

### Janela de coexistência

| Fase | Duração | Comportamento |
|---|---|---|
| **N+1 introduzida** | 0 | SDK passa a parsear V_{N+1}; default continua V_N |
| **Soak** | 6 meses mínimo | Bancos do recebedor migram emissão para V_{N+1}; bancos do pagador atualizam parsers (idempotente — parsear V_N continua funcionando) |
| **Default flip** | — | `ProtocolVersion::current()` aponta para V_{N+1}; novos `encode()` emitem V_{N+1} |
| **Deprecation V_N** | +12 meses | SDK ainda parseia V_N para back-compat. Documentação marca como legacy. |
| **Remoção V_N** | major bump | V_N sai do enum. Bancos que ainda emitirem V_N veem `UnsupportedProtocolVersion`. |

A janela total V_N→remoção é de **≥ 18 meses**. Suficiente para o
parque de dispositivos físicos (POS, ATMs) rotacionar firmware.

## ABI invariants

Estes nunca podem mudar dentro de uma major version da SDK:

- Discriminantes de `ProtocolVersion` (`V1 as u8 == 1`).
- Códigos numéricos de `McpixStatus` (e.g., `UnsupportedProtocolVersion = 15`).
- Layout do `ParsedField` quanto a posição dos campos existentes
  (adicionar campo no final via `#[non_exhaustive]` é OK).
- `PROTOCOL_PREFIX` constante apontando para a versão default emitida.

Quebrar qualquer um destes = major bump da SDK (`0.x` → `1.0` ou
`1.x` → `2.0`), com CHANGELOG explicando migração.

## Testes que ancoram a política

- `version::tests::discriminant_of_v1_is_stable` — `V1 as u8 == 1`.
- `version::tests::all_versions_have_unique_prefixes` — não há
  colisão acidental no enum.
- `transport_field::tests::parse_unknown_version_reports_unsupported_not_prefix`
  — versão futura desconhecida produz erro distinguível.
- `transport_field::tests::parse_distinguishes_foreign_scheme_from_future_version`
  — string não-PIXOFFv* produz `TransportFieldPrefix`.
- `transport_field::tests::parsed_field_carries_version` — `ParsedField.version`
  reflete a versão detectada (não a default emitida).
