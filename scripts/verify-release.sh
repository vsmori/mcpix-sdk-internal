#!/usr/bin/env bash
# Verifica SLSA L3 provenance de todos os artefatos de uma release.
#
# Uso:
#   ./scripts/verify-release.sh v1.0.0 ./dist
#
# Pré-requisito: `slsa-verifier` no PATH
#   go install github.com/slsa-framework/slsa-verifier/v2/cli/slsa-verifier@v2.6.0
#
# Saída:
#   exit 0  → todos os artefatos verificados
#   exit 1  → pelo menos um falhou (mensagem identifica qual)
#
# A verificação NÃO requer chaves locais — usa Sigstore TUF root +
# Rekor transparency log. Apenas conexão de rede para o primeiro acesso
# (cacheada depois).

set -euo pipefail

TAG="${1:?usage: $0 <tag> <dist-dir>}"
DIST="${2:?usage: $0 <tag> <dist-dir>}"
PROVENANCE="$DIST/mcpix-sdk.intoto.jsonl"
SOURCE_URI="github.com/vsmori/mcpix-sdk-internal"

if [[ ! -f "$PROVENANCE" ]]; then
    echo "ERRO: $PROVENANCE não encontrado." >&2
    echo "  Baixe a release com:" >&2
    echo "    gh release download $TAG -R $SOURCE_URI -D $DIST" >&2
    exit 1
fi

if ! command -v slsa-verifier >/dev/null; then
    echo "ERRO: slsa-verifier não encontrado no PATH." >&2
    echo "  Instale com:" >&2
    echo "    go install github.com/slsa-framework/slsa-verifier/v2/cli/slsa-verifier@v2.6.0" >&2
    exit 1
fi

failed=0
verified=0
# Itera por extensões que aparecem na release.
while IFS= read -r -d '' artifact; do
    name="$(basename "$artifact")"
    if slsa-verifier verify-artifact \
        --provenance-path "$PROVENANCE" \
        --source-uri "$SOURCE_URI" \
        --source-tag "$TAG" \
        "$artifact" >/dev/null 2>&1; then
        echo "  OK    $name"
        verified=$((verified + 1))
    else
        echo "  FAIL  $name" >&2
        failed=$((failed + 1))
    fi
done < <(find "$DIST" -type f \
    \( -name "*.so" -o -name "*.dll" -o -name "*.aar" \
       -o -name "*.nupkg" \) -print0)

echo
echo "verificados: $verified  |  falhas: $failed"
if [[ $failed -gt 0 ]]; then
    exit 1
fi
