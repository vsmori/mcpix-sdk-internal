//! Versionamento do protocolo de wire.
//!
//! O mcpix-sdk fixa todas as suas decisões de wire (encoding do campo
//! de transporte, alfabeto de `C₁`/`C₂`, KDF, comprimentos) por *versão*.
//! Esta versão é carimbada no prefixo do campo público de 8 bytes
//! (`PIXOFFv1`, `PIXOFFv2`, ...). O receiver bank que processa o
//! pagamento dispatcha por essa string antes de qualquer parsing
//! posicional — uma versão desconhecida nunca é parseada com regras
//! erradas.
//!
//! ## Por que separar de `transport_field.rs`
//!
//! `transport_field.rs` agora contém *só* o parser **v1**. Para
//! introduzir v2, adicione um `transport_field_v2.rs` paralelo e
//! estenda este enum — o `parse` público dispatcha. Estrutura
//! aditiva: o código v1 nunca precisa mudar para suportar v2.
//!
//! ## Compatibilidade
//!
//! Detalhes em `docs/VERSIONING.md`. Resumo:
//! - Nunca renumeramos um valor existente do enum (`V1 = 1` para
//!   sempre).
//! - Erros legados (`TransportFieldPrefix`) continuam sendo emitidos
//!   apenas quando o prefixo *não pertence* à família `PIXOFFv*`.
//! - Quando o prefixo é `PIXOFFv*` mas o N é desconhecido, emitimos
//!   `UnsupportedProtocolVersion(prefix)` — distinguível em UX:
//!   "atualize seu SDK" vs "essa string não é do nosso protocolo".

use crate::error::McpixError;

/// Família do prefixo da versão. Toda versão começa por estes 7 bytes.
pub const PROTOCOL_PREFIX_FAMILY: &str = "PIXOFFv";
/// Tamanho fixo do prefixo de versão no campo de transporte (8 bytes).
/// Reservar 8 bytes nos permite chegar a `PIXOFFv9` sem mudança de
/// layout; a partir de v10 a versão passa a ocupar 2 bytes do que era
/// padding do SeedId, o que ainda preserva o comprimento total fixo
/// (mas vira *outro* layout, então será V10+ "wave 2" com `prefix`
/// retornando `"PIXOFFvA"`/`B`/`C`/...). Decisão futura.
pub const PROTOCOL_PREFIX_LEN: usize = 8;

/// Versões do protocolo suportadas por este build da SDK.
///
/// **Invariante ABI**: os discriminantes numéricos nunca mudam.
/// Adicionar variantes é compatível; remover ou renumerar é breaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ProtocolVersion {
    V1 = 1,
}

impl ProtocolVersion {
    /// Versão emitida por default ao gerar cobranças. Bumps deste
    /// valor são **breaking change** para integradores que dependem
    /// do wire format default — devem ser feitos com major bump da
    /// SDK e nota no CHANGELOG.
    pub const fn current() -> Self {
        Self::V1
    }

    /// Prefixo ASCII de 8 bytes carimbado no campo de transporte.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::V1 => "PIXOFFv1",
        }
    }

    /// Todas as versões que **este build** da SDK consegue parsear.
    /// Útil para advertise capabilities (futuro: `BankReceiver` expõe
    /// via API a lista, e bancos negociam antes de aceitar tráfego).
    pub const fn all() -> &'static [Self] {
        &[Self::V1]
    }
}

impl core::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.prefix())
    }
}

/// Detecta a versão do protocolo a partir do prefixo do campo.
///
/// Regras de dispatch:
/// - Comprimento < 8: `TransportFieldLength` (já é claramente curto).
/// - Prefixo bate exatamente uma versão conhecida: `Ok(v)`.
/// - Prefixo começa com `PIXOFFv` mas não bate nenhuma versão
///   conhecida: `UnsupportedProtocolVersion(prefix)`. Sinal claro
///   para a UX: "atualize seu SDK".
/// - Prefixo não começa com `PIXOFFv`: `TransportFieldPrefix`. Esse
///   é o caminho para "não é nem nosso protocolo".
pub fn detect(field: &str) -> Result<ProtocolVersion, McpixError> {
    if field.len() < PROTOCOL_PREFIX_LEN {
        return Err(McpixError::TransportFieldLength(field.len()));
    }
    let prefix = &field[..PROTOCOL_PREFIX_LEN];
    for v in ProtocolVersion::all() {
        if prefix == v.prefix() {
            return Ok(*v);
        }
    }
    if prefix.starts_with(PROTOCOL_PREFIX_FAMILY) {
        Err(McpixError::UnsupportedProtocolVersion(prefix.to_string()))
    } else {
        Err(McpixError::TransportFieldPrefix)
    }
}

/// True se o campo carrega prefixo da família `PIXOFFv*`, **mesmo que
/// a versão seja desconhecida por este build**. Utilidade para o
/// payer bank fazer triage: "isto é um instrumento mcpix? deixar
/// passar para o handler que reporta versão; senão, descartar como
/// outro protocolo".
pub fn is_any_version(field: &str) -> bool {
    field.len() >= PROTOCOL_PREFIX_LEN
        && field[..PROTOCOL_PREFIX_LEN].starts_with(PROTOCOL_PREFIX_FAMILY)
}

/// Negocia a versão **mais alta** suportada por ambos os lados.
///
/// `local` é o que **este** build conhece (tipicamente
/// `ProtocolVersion::all()`). `peer` é a lista de prefixos anunciada
/// pelo outro lado via capability endpoint (strings tipo `"PIXOFFv1"`
/// — strings e não enum porque um peer pode anunciar versões que
/// este build nem reconhece como enum, e seria ruim falhar parse
/// nessa direção).
///
/// Retorna `None` se a interseção for vazia — caller decide se isso
/// é fatal (abort transação) ou warn-and-continue (assumir default
/// e ver o que acontece).
///
/// Como o enum `ProtocolVersion` está em ordem ascendente de versão,
/// iteramos `local.iter().rev()` para devolver a **maior versão
/// comum** — política padrão de "negociate the newest both speak".
pub fn negotiate_version(local: &[ProtocolVersion], peer: &[String]) -> Option<ProtocolVersion> {
    local
        .iter()
        .rev()
        .find(|v| peer.iter().any(|s| s == v.prefix()))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_prefix_is_8_bytes() {
        assert_eq!(
            ProtocolVersion::current().prefix().len(),
            PROTOCOL_PREFIX_LEN
        );
    }

    #[test]
    fn detect_recognizes_v1() {
        // Campo completo seria 35 chars; aqui só precisamos do prefixo.
        let field = "PIXOFFv1AAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert_eq!(detect(field).unwrap(), ProtocolVersion::V1);
    }

    #[test]
    fn detect_unsupported_version_distinguishes_from_foreign_scheme() {
        let v2_field = "PIXOFFv2BBBBBBBBBBBBBBBBBBBBBBBBBBB";
        let err = detect(v2_field).unwrap_err();
        match err {
            McpixError::UnsupportedProtocolVersion(p) => assert_eq!(p, "PIXOFFv2"),
            other => panic!("expected UnsupportedProtocolVersion, got {other:?}"),
        }
    }

    #[test]
    fn detect_foreign_scheme_is_prefix_error() {
        let other = "OTHERSCHEMEXXXXXXXXXXXXXXXXXXXXXXXX";
        assert_eq!(detect(other).unwrap_err(), McpixError::TransportFieldPrefix);
    }

    #[test]
    fn detect_too_short_is_length_error() {
        assert!(matches!(
            detect("PIXOFF").unwrap_err(),
            McpixError::TransportFieldLength(6)
        ));
    }

    #[test]
    fn is_any_version_catches_future_versions() {
        // Cliente legado vê v9 no fio — não consegue parsear, mas
        // a triage detecta corretamente que é da nossa família.
        assert!(is_any_version("PIXOFFv9AAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        assert!(is_any_version("PIXOFFv1AAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        assert!(!is_any_version("OTHERSCHEMEXXXXXXXXXXXXXXXXXXXXXXXX"));
    }

    #[test]
    fn all_versions_have_unique_prefixes() {
        // Defesa contra colisão acidental ao adicionar nova versão.
        use std::collections::HashSet;
        let prefixes: HashSet<&str> = ProtocolVersion::all().iter().map(|v| v.prefix()).collect();
        assert_eq!(prefixes.len(), ProtocolVersion::all().len());
    }

    #[test]
    fn discriminant_of_v1_is_stable() {
        // Anchor da invariante ABI — se este teste quebrar, alguém
        // mexeu no `#[repr(u8)]` enum.
        assert_eq!(ProtocolVersion::V1 as u8, 1);
    }

    #[test]
    fn negotiate_picks_v1_when_peer_supports_it() {
        let local = [ProtocolVersion::V1];
        let peer = vec!["PIXOFFv1".to_string()];
        assert_eq!(negotiate_version(&local, &peer), Some(ProtocolVersion::V1));
    }

    #[test]
    fn negotiate_returns_none_when_disjoint() {
        // Peer só fala v99 (futuro); este build só conhece V1.
        let local = [ProtocolVersion::V1];
        let peer = vec!["PIXOFFv9".to_string()];
        assert_eq!(negotiate_version(&local, &peer), None);
    }

    #[test]
    fn negotiate_picks_highest_common_even_when_peer_has_more() {
        // Peer suporta v1 + (futurística) v9. Este build só conhece
        // v1 → escolhe v1, único comum.
        let local = [ProtocolVersion::V1];
        let peer = vec!["PIXOFFv9".to_string(), "PIXOFFv1".to_string()];
        assert_eq!(negotiate_version(&local, &peer), Some(ProtocolVersion::V1));
    }

    #[test]
    fn negotiate_ignores_unknown_peer_versions() {
        // Strings desconhecidas pelo enum não quebram a negociação —
        // só não contam para interseção.
        let local = [ProtocolVersion::V1];
        let peer = vec!["GIBBERISH".to_string(), "PIXOFFv1".to_string()];
        assert_eq!(negotiate_version(&local, &peer), Some(ProtocolVersion::V1));
    }

    #[test]
    fn negotiate_empty_peer_returns_none() {
        // Peer não anunciou nada — sem versão para negociar.
        let local = [ProtocolVersion::V1];
        assert_eq!(negotiate_version(&local, &[]), None);
    }
}
