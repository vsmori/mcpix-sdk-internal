# Documentação técnica — mcpix-sdk

Material de apoio ao depósito PCT. Conteúdo escrito para revisor técnico
externo, não para integrador.

## Índice

| Documento | Propósito |
|---|---|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | Visão geral da arquitetura, módulos e responsabilidades |
| [PROTOCOL.md](./PROTOCOL.md) | Especificação do protocolo + sequence diagrams |
| [CRYPTO.md](./CRYPTO.md) | Especificação criptográfica formal |
| [THREAT_MODEL.md](./THREAT_MODEL.md) | Modelo de ameaças, atores, capacidades, mitigações |
| [PCT_CLAIMS_MAPPING.md](./PCT_CLAIMS_MAPPING.md) | Mapeamento reivindicação → código |
| [GLOSSARY.md](./GLOSSARY.md) | Glossário de termos do protocolo |
| [adr/](./adr/) | Architectural Decision Records (9 ADRs) |

## Audiência

- **Examinador técnico do pedido PCT**: PROTOCOL.md + CRYPTO.md +
  PCT_CLAIMS_MAPPING.md são os documentos primários. THREAT_MODEL.md
  suporta as reivindicações relativas a segurança.
- **Auditor de segurança independente**: THREAT_MODEL.md + ADRs +
  CRYPTO.md cobrem decisões e justificativas.
- **Integrador / parceiro técnico**: ARCHITECTURE.md + GLOSSARY.md +
  rustdoc gerado por `cargo doc --workspace --no-deps`.

## Convenções

- **Diagramas**: Mermaid embarcado em Markdown — renderiza no GitHub,
  GitLab e na maioria dos editores. Sem dependência de ferramenta externa.
- **Referências a código**: notação `path:linha` (ex.
  `crates/mcpix-core/src/crypto.rs:71`).
- **Identificadores**: nomes de funções, tipos e crates em `monospace`.
- **Terminologia genérica**: o material evita marcas comerciais
  registradas de terceiros. Quando referência a formato externo é
  necessária (ex. faixa de comprimento `[a-zA-Z0-9]{26,35}` herdada de
  padrão financeiro brasileiro), citamos apenas a estrutura, sem o
  nome do padrão.

## Estado de cobertura

| Tópico | Documento | Status |
|---|---|---|
| Algoritmo de derivação `(C₁, C₂)` | CRYPTO.md §2 | escrito |
| Encadeamento C₁ → C₂ | CRYPTO.md §3, ADR-001 | escrito |
| Comparação em tempo constante | ADR-003 | escrito |
| Substituição institucional | PROTOCOL.md §4 | escrito |
| Defesa contra replay | THREAT_MODEL.md §4.3 | escrito |
| Modos de contador (sequencial / quantizado) | ADR-007 | escrito |
| Cadeia de confiança do binário | THREAT_MODEL.md §5 | escrito |
| Portabilidade para microcontroladores | ADR-009 | escrito |
