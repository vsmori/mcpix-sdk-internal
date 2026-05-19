# Seed corpus do fuzzing

Esta pasta contém entradas de seed para cada fuzz target, descobertas em
execuções anteriores do libfuzzer. Está **versionada em git**, com dois
propósitos:

1. **Aceleração de runs futuros** — libfuzzer começa muito mais rápido
   quando há corpus inicial; sem isso, cada CI run do `fuzz.yml`
   precisa redescobrir coverage do zero.

2. **Suite de regressão determinística** — o teste
   `crates/mcpix-core/tests/corpus_replay.rs` itera **todo** o corpus a
   cada push de CI e roda cada entrada pelo seu target. Significado: se
   alguém quebrar a invariante "parser não capota" em qualquer destas
   entradas, a CI quebra no PR, **sem precisar de nightly nem rodar
   libfuzzer**.

## Layout

```
fuzz/corpus/
├── README.md                       ← este arquivo
├── fuzz_transport_parse/           ← seeds para transport_field::parse
├── fuzz_sums_line/                 ← seeds para signature::parse_sums_line
└── fuzz_verify_combined/           ← seeds para signature::verify_combined
```

Os arquivos têm nomes hash (gerados pelo libfuzzer) e bytes
opacos. **Não edite manualmente.** Para adicionar novos seeds:

- **Seeds positivos** (cobrir caminhos novos): rode o fuzzer e ele
  descobre sozinho — apenas comite o diff de `corpus/`.
- **Casos manualmente curados** (e.g. crash reproduzível ou edge case
  conhecido): use `fuzz/regression/<target>/<descritivo>` em vez de
  `corpus/`. O replay percorre ambos.

## Como rodar o replay localmente

```bash
# Via cargo test (recomendado — bate o que CI roda):
cargo test -p mcpix-core --test corpus_replay

# Via xtask (mesmo resultado, output mais legível):
cargo xtask fuzz-replay
```

## Política

- **Tamanho**: cap informal de ~5 MB total. Acima disso, considerar
  truncar inputs duplicados ou usar minimização (`cargo fuzz cmin`).
- **Crashes**: nunca devem ficar dormindo aqui. Quando o `fuzz.yml`
  detectar crash em `fuzz/artifacts/`, triage manual → fix ou move
  para `regression/` (caso o fix exija mudança de invariante e o input
  agora seja válido como caso edge).
