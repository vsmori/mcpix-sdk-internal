# Casos de regressão (curados)

Diferença vs `fuzz/corpus/`: aqui são entradas **manualmente
selecionadas** — tipicamente reproduções de crashes que já foram
encontrados e corrigidos, ou edge cases descobertos fora do fuzzing
(revisão de código, relatos de produção, etc.).

## Quando adicionar uma entrada

Sempre que um bug for corrigido cujo trigger pode ser representado como
input opaco para um dos targets. Workflow:

1. Crash detectado (CI do `fuzz.yml` ou run local).
2. Copia o arquivo de `fuzz/artifacts/<target>/<hash>` para
   `fuzz/regression/<target>/<descritivo>`.
3. Renomeia para algo legível: `crash_overflow_on_15char_prefix`,
   `edge_zero_byte_inside_seed_id`, etc.
4. Fix o código que causou o crash.
5. Roda `cargo test -p mcpix-core --test corpus_replay` — deve passar
   agora (e quebraria se o fix regredisse).
6. Comita fix + arquivo de regressão no mesmo PR.

## Estrutura

```
fuzz/regression/
├── README.md
├── fuzz_transport_parse/
├── fuzz_sums_line/
└── fuzz_verify_combined/
```

Vazias enquanto não houver findings. O `.gitkeep` em cada subpasta
mantém a estrutura presente no clone — quando o primeiro caso entrar,
remova o `.gitkeep`.
