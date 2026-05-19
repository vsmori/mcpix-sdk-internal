//! Tipos receiver-only com tamanhos fixos (stack-only, sem `alloc`).
//!
//! Espelham as constantes de `mcpix-core::types` mas com armazenamento via
//! `[u8; N]` em vez de `String`/`Vec`. Cross-validado em runtime contra o
//! núcleo host em `tests/cross_validate.rs`.

use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

pub const SEED_LEN: usize = 32;
pub const SEED_ID_MAX_LEN: usize = 16;
pub const C1_LEN: usize = 11;
pub const C2_LEN: usize = 11;

/// Erros do subset embarcado. Sem `String` interna — todos os variants têm
/// payload `'static` ou numérico. `Debug` suficiente para logging via
/// `defmt::Debug2Format` ou similar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedError {
    /// SeedId vazio ou maior que `SEED_ID_MAX_LEN`.
    SeedIdLength(usize),
    /// SeedId contém caractere fora de `[a-zA-Z1-9]`.
    SeedIdCharset,
    /// Buffer de saída tem tamanho diferente do esperado.
    BufferLen { expected: usize, got: usize },
    /// Conteúdo do campo de transporte não obedece ao layout.
    TransportFieldLayout,
}

/// Semente compartilhada com o banco recebedor. `[u8; 32]` na stack;
/// zeroizada ao sair de escopo.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Seed(pub(crate) [u8; SEED_LEN]);

impl Seed {
    pub const fn from_bytes(bytes: [u8; SEED_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SEED_LEN] {
        &self.0
    }
}

impl fmt::Debug for Seed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seed(REDACTED)")
    }
}

/// SeedId — string ASCII curta, validada via construtor. Internamente
/// `heapless::Vec<u8, 16>` para evitar dependência de alloc.
#[derive(Clone, PartialEq, Eq)]
pub struct SeedId {
    buf: heapless::Vec<u8, SEED_ID_MAX_LEN>,
}

impl SeedId {
    pub fn new(s: &str) -> Result<Self, EmbedError> {
        let bytes = s.as_bytes();
        if bytes.is_empty() || bytes.len() > SEED_ID_MAX_LEN {
            return Err(EmbedError::SeedIdLength(bytes.len()));
        }
        if !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() && *b != b'0')
        {
            return Err(EmbedError::SeedIdCharset);
        }
        let mut buf = heapless::Vec::new();
        buf.extend_from_slice(bytes)
            .map_err(|_| EmbedError::SeedIdLength(bytes.len()))?;
        Ok(Self { buf })
    }

    pub fn as_str(&self) -> &str {
        // SAFETY-equivalent without unsafe: `new` valida UTF-8 ASCII.
        core::str::from_utf8(&self.buf).expect("SeedId is ASCII by construction")
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl fmt::Debug for SeedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SeedId({})", self.as_str())
    }
}

/// Código de cobrança C₁ — alfanumérico ASCII de tamanho fixo.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct C1(pub(crate) [u8; C1_LEN]);

impl C1 {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).expect("C1 alphanumeric ASCII by construction")
    }

    pub fn as_bytes(&self) -> &[u8; C1_LEN] {
        &self.0
    }
}

impl fmt::Debug for C1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C1({})", self.as_str())
    }
}

/// Código de confirmação C₂ — material sensível, zeroizado.
///
/// `PartialEq`/`Eq` são providos para conveniência em testes e em
/// serialização/desserialização (CRC + record equality). **Para
/// comparar com material apresentado em runtime, use `verify_c2()`
/// — esta sim opera em tempo constante.**
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct C2(pub(crate) [u8; C2_LEN]);

impl C2 {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).expect("C2 alphanumeric ASCII by construction")
    }

    pub fn as_bytes(&self) -> &[u8; C2_LEN] {
        &self.0
    }
}

impl fmt::Debug for C2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("C2(REDACTED)")
    }
}
