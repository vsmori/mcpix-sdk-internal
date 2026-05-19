# mcpix-sdk — demo browser (WebAssembly)

Demo single-page que executa o SDK inteiro em wasm e simula os dois bancos
(recebedor + pagador) em memória. ~80 KB de wasm, sem polyfills, sem rede.

## Build

```bash
cargo xtask build-wasm
```

Esse comando:
1. Compila `mcpix-wasm` para `wasm32-unknown-unknown` em release;
2. Roda `wasm-bindgen --target web` produzindo `pkg/` ao lado deste README.

Pré-requisitos:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.121
```

## Run

A página é estática. Sirva o diretório local com qualquer servidor HTTP
(necessário porque módulos ES exigem MIME `application/javascript`):

```bash
cd examples/web-demo
python3 -m http.server 8080
# abre http://localhost:8080/
```

## Fluxo da demo

1. **Cadastrar semente** (recebedor) — gera `Seed` de 32 bytes via
   `crypto.getRandomValues`.
2. **Gerar cobrança** (recebedor) — deriva `(C₁, C₂)` a partir de
   `(Seed, T)` via HMAC-SHA256; armazena `C₂` localmente; mostra o campo
   de transporte público de 35 chars.
3. **Consultar semente e recuperar C₂** (pagador) — parseia o campo,
   consulta a `Seed` do recebedor (em produção via mTLS — aqui acesso
   direto à mesma memória), reconstrói o **mesmo `C₂`** offline.
4. **Validar comprovante** (recebedor) — recebe o `C₂` apresentado pelo
   pagador, compara com o `C₂` retido. `Valid` → marca como consumido.
5. **Replay** — tentar validar de novo retorna `Replay` (defesa de
   reuso ancorada no store local).

## Por que isso prova o protocolo

- **Substituição institucional**: `retained_c2` (recebedor) ≡
  `recovered_c2` (pagador). O recebedor nunca transmitiu C₂; o pagador
  derivou com a `Seed` que veio do recebedor.
- **Anti-replay**: store marca consumido após o primeiro `Valid`.
- **Anti-tampering**: alterar 1 bit no campo de transporte produz `C₁`
  diferente → `C₂` derivado é outro → validação retorna `Mismatch`.

## O que NÃO está coberto

- Persistência (SQLite/EEPROM/storage) — `InMemorySeedStore` zera ao
  recarregar a página.
- mTLS entre bancos — substituído pelo acesso direto à `Seed` no mesmo
  módulo wasm (mostra o conteúdo da consulta, mas não o canal).
- Counter quantizado por tempo — usamos contador monotônico
  in-memory (`InMemoryCounter`). A SDK full suporta `TimestampCounter`,
  fora do escopo desta demo.
