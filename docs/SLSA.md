# SLSA L3 — verificação de provenance

Cada release oficial do mcpix-sdk publica, além dos artefatos binários
(`.so`, `.dll`, `.aar`, `.nupkg`, `MCPixSDKFFI.xcframework`), um arquivo
de **SLSA Provenance v1** assinado:

```
mcpix-sdk.intoto.jsonl
```

Ele é gerado pelo
[`slsa-github-generator`](https://github.com/slsa-framework/slsa-github-generator)
em um runner GitHub Actions **separado** do build, com:

- **Assinatura keyless** via Sigstore Fulcio (certificado X.509 efêmero
  vinculado ao OIDC token do GitHub Actions — não há chave privada
  persistente que possa vazar).
- **Inclusão em transparency log público** (Rekor): qualquer adulteração
  posterior do attestation é visível globalmente.
- **Predicate `slsa-provenance` v1** descrevendo: commit do repo fonte,
  workflow path (`.github/workflows/release.yml`), invocation parameters,
  builder identity, materials.

## Threat model coberto

Esta provenance fecha o item §5.3 do
[`THREAT_MODEL.md`](THREAT_MODEL.md) — **comprometimento do CI**:

| Cenário | Mitigação |
|---|---|
| Atacante modifica um job para inserir backdoor | Fulcio cert assinante carrega o workflow path no claim subject; alteração quebra a verificação |
| Atacante reutiliza assinatura de release antigo num artefato novo | Subject digest no in-toto vincula bit-exato o `.so` ao predicate; troca de bytes = `slsa-verifier` rejeita |
| Atacante substitui o `.intoto.jsonl` depois da publicação | Rekor inclusion proof exige consistência com o log público — divergência detectável |
| Atacante compromete um runner GitHub | Não consegue forjar OIDC token do GitHub IdP sem comprometer o próprio GitHub |
| Atacante mantém validade da assinatura legada (chave Ed25519 vazada) | Assinatura legada é apenas conveniência; verificação canônica é via SLSA |

## Verificação (consumidor / banco integrador)

### Pré-requisitos

```bash
# Instala o verificador oficial (Go binary, sem deps).
go install github.com/slsa-framework/slsa-verifier/v2/cli/slsa-verifier@v2.6.0
```

### Comando canônico

```bash
slsa-verifier verify-artifact \
  --provenance-path  mcpix-sdk.intoto.jsonl \
  --source-uri       github.com/vsmori/mcpix-sdk-internal \
  --source-tag       v1.0.0 \
  ./libmcpix_ffi.so
```

O verificador checa **localmente**, sem rede além de Rekor/Fulcio:

1. **Assinatura Fulcio**: cert chain válido até a root da Sigstore TUF
   repository.
2. **Claim de identidade do builder**: o subject do cert é
   `https://github.com/vsmori/mcpix-sdk-internal/.github/workflows/release.yml@refs/tags/v1.0.0`
   (ou ref equivalente). Workflow path diferente = reject.
3. **Inclusão em Rekor**: prova de Merkle válida + entry timestamp
   anterior à data atual.
4. **Subject digest no predicate**: SHA-256 do binário fornecido
   confere com o `subjects[].digest.sha256` do `.intoto.jsonl`.
5. **Source repo e ref**: `materials[0]` aponta para
   `github.com/vsmori/mcpix-sdk-internal@<commit>` e a tag bate.

Se qualquer um falhar, exit code != 0 e o artefato **não deve ser
carregado**.

### Script auxiliar

Para verificar todos os artefatos de uma só vez:

```bash
# Em scripts/verify-release.sh
./scripts/verify-release.sh v1.0.0 ./downloaded-artifacts/
```

### Verificação em CI (consumidor)

Integradores podem plugar um job que falhe o pipeline se a verificação
não passar — exemplo para um pipeline Kotlin que consome o `.aar`:

```yaml
- name: download artifacts
  run: |
    gh release download v1.0.0 -R vsmori/mcpix-sdk-internal \
      -p '*.aar' -p 'mcpix-sdk.intoto.jsonl'

- name: install slsa-verifier
  uses: slsa-framework/slsa-verifier/actions/installer@v2.6.0

- name: verify provenance
  run: |
    slsa-verifier verify-artifact \
      --provenance-path  mcpix-sdk.intoto.jsonl \
      --source-uri       github.com/vsmori/mcpix-sdk-internal \
      --source-tag       v1.0.0 \
      mcpix-sdk-android.aar
```

## Por que SLSA L3 e não L4

L3 cobre: build hospedado, provenance autenticada, build isolado,
provenance não-falsificável. Atendido.

L4 exige adicionalmente **build hermético + reproduzível bit-exato**.
Rust com `Cargo.lock` lockado + sysroot fixo nos runners GitHub é
*muito próximo* mas não estritamente hermético (network access durante
`cargo fetch`; algumas crates têm `build.rs` que consultam variáveis de
ambiente). Migrar para L4 requer:

- Mirror local de crates.io (`cargo --offline` no build).
- Eliminação de `build.rs` não-determinísticos (poucos no nosso grafo
  — `ed25519-dalek` é o principal candidato a auditoria).
- Comparação cross-runner do binário (dois runners, mesmo hash).

Roadmap futuro — fora do escopo desta entrega.
