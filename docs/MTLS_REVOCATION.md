# mTLS — revogação (CRL + OCSP stapling)

A SDK aceita verificação de revogação em ambos os lados do canal
banco-pagador ↔ banco-recebedor. Esta página resume o modelo de
operação e os pontos de configuração.

## Cenários cobertos

| Quem | Material | Efeito |
|---|---|---|
| Servidor (recebedor) | `ServerTlsConfig::with_client_crls(crls_pem)` | Rejeita **client certs** revogados pela CA da federação |
| Servidor | `ServerTlsConfig::with_stapled_ocsp(der)` | Anexa OCSP response da CA no `ServerHello` (TLS Certificate Status Extension); cliente que valida stapling detecta revogação |
| Cliente (pagador) | `MtlsClientMaterial::with_server_crls(crls_pem)` | Rejeita **server certs** revogados, mesmo se aceitos pela CA |

CRLs vazias (default) preservam o comportamento legado — backwards
compatible para integradores que ainda não publicam CRLs.

## Fechamento de §6.5 (revogação)

A versão anterior do `THREAT_MODEL.md` listava revogação como
*"limite atual: rotação manual via re-emissão"*. Agora a SDK suporta:

1. **CRL fim-a-fim** — biblioteca rustls valida que o serial number do
   cert apresentado não está numa CRL assinada pela CA, com janela
   `thisUpdate..nextUpdate` válida e assinatura íntegra. CRL expirada
   = builder error (força rotação).
2. **OCSP stapling** server-side — operador faz cron pull da OCSP
   responder da CA, carimba na ServerConfig, cliente verifica.

O que **continua fora do escopo**:

- **Live OCSP query como hook do handshake TLS**. A SDK provê o
  módulo `mcpix-bank-receiver::ocsp` (feature `ocsp`) com
  `OcspChecker` para consulta **out-of-band**, executada antes de
  operações sensíveis — não em cada handshake. Veja
  [Live OCSP](#live-ocsp-query-out-of-band) abaixo.
- **Verificação criptográfica da assinatura na OcspResponse** — a
  Phase 1 do módulo `ocsp` parseia status mas delega validação da
  assinatura ao integrador, que tem a CA chain do mTLS. Phase 2
  fechará isso quando tooling de delegate-signing estiver pronto.
- **CRL distribution points dinâmicos** — não fazemos parse da extensão
  `crlDistributionPoints` do cert para descobrir onde baixar a CRL.
  Operador fornece o PEM consolidado.

## Geração / distribuição da CRL

CA emite a CRL periodicamente (típico: 24h). Estrutura mínima:

```text
Certificate Revocation List (Version 2)
  Issuer:        CN=mcpix-federation-ca
  thisUpdate:    2026-05-18T00:00:00Z
  nextUpdate:    2026-05-19T00:00:00Z
  Revoked:
    Serial 0x1f2c  RevocationDate 2026-05-17T15:30Z  Reason KeyCompromise
    Serial 0x4ab8  RevocationDate 2026-05-17T16:02Z  Reason CessationOfOperation
  Signature:     RSA-SHA256(...)
```

Formato no fio: **PEM** (`-----BEGIN X509 CRL-----`) ou DER.
`mtls::load_crls(pem)` aceita ambos via `rustls_pemfile::crls`.

### Refresh

```rust
// Pseudocódigo do hot-reload server-side
loop {
    let new_crl = fetch_crl_from_ca("https://ca.federation.example/crl.pem")?;
    if new_crl != current_crl {
        let cfg = ServerTlsConfig::new(server_cert, server_key, client_ca)
            .with_client_crls(&new_crl);
        let new_server_config = build_server_config_full(&cfg)?;
        axum_server::reload(new_server_config);  // pseudo
        current_crl = new_crl;
    }
    tokio::time::sleep(Duration::from_secs(3600)).await;
}
```

Janela aceita: a SDK rejeita CRL com `nextUpdate` no passado. Operador
deve garantir que o pull cobre o intervalo confortavelmente (recomendado
≥4× a frequência do `nextUpdate`).

## OCSP stapling

```rust
let ocsp_der = fetch_ocsp_from_ca(&server_cert)?;  // request OCSP da CA
let cfg = ServerTlsConfig::new(server_cert, server_key, client_ca)
    .with_stapled_ocsp(&ocsp_der);
let server_config = build_server_config_full(&cfg)?;
```

O `ocsp_der` deve ser a DER da OCSPResponse retornada pela responder OCSP
da CA. rustls anexa no handshake; cliente com `WebPkiServerVerifier`
valida assinatura e `producedAt` ≤ agora ≤ `nextUpdate`.

**Limite**: rustls ainda não suporta OCSP stapling client-side (cliente
não valida stapling automaticamente sem `WebPkiServerVerifier`
configurado explicitamente). O caminho `MtlsClientMaterial` com CRLs
ativadas usa `WebPkiServerVerifier`, então valida stapling — caminho
legado (sem CRLs) usa o verifier default do reqwest, que historicamente
não verifica stapling.

## Live OCSP query (out-of-band)

Feature `ocsp` (cargo `--features ocsp,http-client`). Útil quando
CRL pode estar minutos desatualizada e a operação merece o custo de
+1 RTT contra o responder da CA.

```rust
use mcpix_bank_receiver::ocsp::{OcspChecker, OcspStatus};

let client = reqwest::blocking::Client::new();
let checker = OcspChecker::new(&client, "http://ocsp.federation-ca.example/");

// Antes de iniciar uma transação sensível:
match checker.check(&server_cert_pem, &issuer_cert_pem)? {
    OcspStatus::Good => { /* prossegue */ }
    OcspStatus::Revoked { reason_code } => {
        return Err(/* aborta — cert revogado */);
    }
    OcspStatus::Unknown => {
        // Política: fail-closed (recomendado) ou fail-open (apenas log).
        return Err(/* responder não conhece o cert */);
    }
}
```

**Phase 1 (entregue):** request builder, transport HTTP POST,
response parser. 9 testes cobrindo wire round-trip, malformed bytes,
responder unreachable.

**Phase 2 (próximas sessões):** verificação criptográfica da
assinatura da `OcspResponse` contra a CA. Hoje a integridade da
response depende do canal cliente↔responder. Workaround temporário:
use HTTPS para o responder URL (validado pelos system roots ou pelo
seu CA bundle), o que reduz a janela a um TOFU vs MITM ativo na
hierarquia de root certs públicos.

## Tabela de risco residual

| Cenário | Coberto? |
|---|---|
| Cert do server roubado e mTLS pode chegar a um peer comprometido | sim, CRL + (opcional) OCSP stapling rejeita |
| Cert do client da instituição-A vazado, instituição usado para personificar | sim, CRL no server rejeita |
| Atacante intercepta CRL no caminho operador→server | parcial — CRL é PEM assinada pela CA; tampering invalida assinatura mas operador deve detectar 404/erro de pull |
| CRL antiga aceita após `nextUpdate` | não — rustls rejeita |
| Atacante que comprometeu a CA emite "CRL vazia" e desbloqueia certs revogados | fora do escopo — comprometer a CA é game over de qualquer PKI |

## Testes que ancoram o comportamento

- `mtls_rejects_revoked_client_cert` — CRL no servidor; client cert revogado → handshake fail
- `mtls_accepts_non_revoked_client_when_crl_active` — CRL ativa porém vazia; client cert válido → handshake OK (sem falso positivo)
- `client_rejects_revoked_server_cert_via_crl` — CRL no cliente; server cert revogado → handshake fail

Todos no arquivo `crates/mcpix-bank-receiver/tests/mtls_e2e.rs`.
