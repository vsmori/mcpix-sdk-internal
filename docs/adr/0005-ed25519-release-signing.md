# ADR-0005: Ed25519 com pub key embarcada para assinatura de release

## Status

Aceito — implementado em S4. Cobertura em
`tests/integrity_against_dist.rs` (3 testes signed_manifest).

## Contexto

A camada anterior (S3) carimba o hash SHA-256 do binário esperado em
`MCPIX_EXPECTED_SHA256`. Detecta substituição/patch ingênuo, mas é
trivialmente defeated por atacante que recompile com hash novo
carimbado.

Para fechar a cadeia de confiança, precisa-se de um trust anchor que
o atacante **não controle**: uma chave privada de release mantida
em secret e cuja pública seja embarcada no binário em compile time.

Considerações:

- Compatibilidade com `no_std` (embarcado precisa verificar firmware).
- Footprint de código ≤ 30 KB (cabe em Cortex-M0).
- Verificação offline (sem contato com Sigstore/Rekor) — federação
  fechada da especificação.
- Operações criptográficas auditadas.

## Decisão

Adotar **Ed25519** ([ed25519-dalek](https://crates.io/crates/ed25519-dalek))
como esquema de assinatura. Workflow:

1. `cargo xtask gen-release-key` produz par `(priv, pub)`. Escreve
   `crates/mcpix-core/trusted_keys/release.pub` (32 bytes raw) e
   `target/release-key.priv` (gitignored). Imprime priv em hex uma
   vez para gravação em secret.
2. Em release, CI invoca `cargo xtask sign-artifacts` com a priv key
   no env var `MCPIX_SIGN_PRIVKEY_HEX`. Produz `dist/SHA256SUMS.sig`
   (64 bytes raw).
3. Em runtime, `verify_self()` lê `SHA256SUMS` + `SHA256SUMS.sig`
   adjacentes ao binário, valida assinatura contra `RELEASE_PUBKEY`
   (`include_bytes!("../trusted_keys/release.pub")`) e localiza o
   hash do próprio binário no manifesto.

## Alternativas consideradas

### A1. RSA-PSS / RSA-PKCS1v15

**Por que não.** Chave RSA-3072 = 384 bytes pub key + ~400 bytes
assinatura. Ed25519 = 32 + 64 bytes. Para uso embarcado, o tamanho
importa. Verificação RSA também é mais lenta (~100x em Cortex-M).

### A2. ECDSA P-256

**Por que não.** Suporte equivalente, mas API mais traiçoeira
(ECDSA exige nonce único e aleatório; reuso de nonce vaza priv key).
Ed25519 é determinístico por design — não há risco operacional de
"esqueci de randomizar".

### A3. Sigstore cosign keyless (OIDC + Rekor)

**Por que não.** Verificação exige acesso a Rekor transparency log
e Fulcio CA. Cenário offline (recebedor sem rede) não consegue
validar. Bom para builds, ruim para runtime check em dispositivo
isolado.

### A4. GPG/OpenPGP

**Por que não.** Pubkey ring + cadeia de confiança via key servers.
Mais infraestrutura. crate `pgp` em Rust funciona mas pesa ~80 KB
de código — não cabe confortavelmente em MCU.

### A5. Hash do binário sem assinatura (S3 sozinho)

**Por que não.** Atacante que controla o build pode trocar tanto o
binário quanto o hash carimbado. Sem trust anchor externo (pub key),
a cadeia é circular.

## Consequências

**Positivas:**

- Pub key embarcada em compile time via `include_bytes!` — substituir
  exige re-compilar o crate. Atacante que controla apenas o
  filesystem do dispositivo não consegue rotar.
- Verificação rápida: ~150μs em Cortex-M4F para Ed25519 verify.
- Footprint: ed25519-dalek com `default-features = false, std`
  adiciona ~16 KB ao binário — aceitável.
- Compatível com `no_std` (mcpix-embed pode no futuro adicionar
  verificação de firmware).

**Negativas:**

- Comprometimento da priv key requer rotação coordenada: novo
  release.pub commitado + tag de breaking-compat. Documentado em
  `crates/mcpix-core/trusted_keys/README.md`.
- Sem revogação automática: clientes em campo continuam confiando
  na pub key anterior até receberem update do binário.

## Validação

| Cenário | Teste | Resultado esperado |
|---|---|---|
| SUMS + sig + binário corretos | `signed_manifest_verifies_against_release_pubkey` | `Verified` |
| Hash em SUMS não bate com binário real | `signed_manifest_detects_swapped_binary` | `Tampered` |
| 1-byte flip no SUMS | `signed_manifest_detects_tampered_sums` | `InvalidSignature` |
| Sem SUMS.sig em release build | `verify_self` legacy path | `Tampered` (após policy strict) |

## Rotação

Documentado em `crates/mcpix-core/trusted_keys/README.md`. Procedimento:

```
rm crates/mcpix-core/trusted_keys/release.pub
cargo xtask gen-release-key      # imprime priv hex
# atualizar secret MCPIX_SIGN_PRIVKEY_HEX no CI
git commit "rotate release signing key"
```

## Referências

- Bernstein, Duif, Lange, Schwabe, Yang. "High-speed high-security
  signatures" (Ed25519 paper, 2011).
- RFC 8032 — Edwards-Curve Digital Signature Algorithm (EdDSA).
- The Update Framework (TUF) — modelo de cadeia de confiança que
  inspirou a estrutura aqui.
