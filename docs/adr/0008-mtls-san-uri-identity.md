# ADR-0008: Identidade da instituição via SAN URI no cert mTLS

## Status

Aceito — implementado em S8.

## Contexto

A consulta inter-institucional `BP → BR` (banco do pagador buscando
semente no banco do recebedor) precisa de:

1. **Confidencialidade + integridade** do canal — TLS clássico.
2. **Autenticação mútua** — ambos os lados provam posse de chave
   privada de cert assinado por CA da federação.
3. **Identidade estruturada** da instituição requerente — para
   logging, auditoria, autorização granular futura.

A S7 usava o header HTTP `X-Institution-Id` como placeholder, o que
é trivialmente forjável.

## Decisão

**Identidade carregada no SAN URI do client cert** no formato:

```
urn:mcpix:institution:<institution_id>
```

Exemplo: `urn:mcpix:institution:BANK_PAYER_42`.

Implementação:

- `mtls_server::build_server_config` configura
  `WebPkiClientVerifier` em modo obrigatório contra a root CA da
  federação. Cliente sem cert válido nunca completa handshake.
- `mtls::extract_institution_id` parseia o cert DER (via crate
  `x509-parser`) e busca SAN URIs com prefixo
  `urn:mcpix:institution:`. Fallback para CN se ausente.
- Caller pode invocar o helper diretamente sobre cert obtido por
  outras vias (ex. header `X-Forwarded-Client-Cert` propagado por
  envoy/nginx no termination layer).

## Alternativas consideradas

### A1. CN do subject

`CN=BANK_PAYER_42`

**Por que não.** RFC 6125 desencoraja CN para validação de identidade
desde 2011 — historicamente conflita com hostname (servidor) vs
identidade. SAN é o local correto. Mantemos CN apenas como fallback
para PKIs legadas.

### A2. SAN DNS

`DNS:bank-payer-42.mcpix.fed`

**Por que não.** SAN DNS é semanticamente "hostname". Reutilizar
para identidade de instituição é abuso semântico — confunde
auditores e ferramentas (ex. validadores de cert que aplicam
constraints DNS-name).

### A3. Header HTTP + JWT

Manter `X-Institution-Id` mas exigir JWT assinado contendo a identidade.

**Por que não.** Adiciona uma camada (JWS) ao que já está coberto
pela camada TLS. mTLS é o padrão para autenticação mútua entre
serviços de federações fechadas; JWT é melhor para auth de usuário
em web apps. Não compor desnecessariamente.

### A4. OID customizado em X.509 extension

Criar OID privado `1.3.6.1.4.1.<XXX>.1` carregando o ID.

**Por que não.** Exige registro IANA do OID, mais ferramentas
precisariam parsear extension custom. SAN URI usa estrutura
padronizada (URN) que qualquer parser X.509 já lê.

## Consequências

**Positivas:**

- Identidade carregada no canal mTLS — não pode ser forjada sem
  cert privado da instituição.
- Formato URN namespace-friendly: `urn:mcpix:institution:` deixa
  espaço para outros tipos (`urn:mcpix:gateway:`,
  `urn:mcpix:auditor:`) sem conflito.
- `extract_institution_id` é função pura, testável sem montar mTLS
  completo (tests usam certs gerados in-process via `rcgen`).

**Negativas:**

- Requer PKI emitindo certs com SAN URI. PKIs corporativas legadas
  às vezes têm tooling que ignora SAN — fallback para CN existe
  exatamente para esses casos.
- Sem revogação (OCSP/CRL) ainda. Documentado em
  [THREAT_MODEL.md §6.5](../THREAT_MODEL.md#65-revogação).

## Validação

| Cenário | Teste | Resultado |
|---|---|---|
| Cert com SAN URI bem formado | `mtls::tests::extracts_institution_from_san_uri` | extrai `BANK_PAYER_42` |
| Cert só com CN | `mtls::tests::falls_back_to_cn_when_no_san_uri` | extrai CN |
| SAN URI com prefixo errado, com CN | `mtls::tests::ignores_san_uri_without_correct_prefix_and_falls_back_to_cn` | usa CN |
| Cert válido na conexão mTLS | `mtls_e2e::mtls_round_trip_succeeds_with_valid_client_cert` | request OK |
| Sem cert | `mtls_e2e::mtls_rejects_client_without_cert` | handshake fail |
| CA não-confiada | `mtls_e2e::mtls_rejects_client_from_untrusted_ca` | handshake fail |

## Referências

- RFC 6125 — Representation and Verification of Domain-Based
  Application Service Identity.
- RFC 4985 — Internet X.509 Public Key Infrastructure Subject
  Alternative Name for Expression of Service Name.
- RFC 8141 — Uniform Resource Names (URNs).
