//! Tipos de domínio do protocolo.
//!
//! Todos os tipos são `Clone` por construção barata (bytes fixos), porém os que
//! contêm material secreto implementam `ZeroizeOnDrop` para reduzir janela de
//! exposição na memória.

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::McpixError;

/// Tamanho fixo do material de semente em bytes (256 bits).
pub const SEED_LEN: usize = 32;

/// Tamanho do SeedId no campo de transporte (após codificação alfanumérica).
pub const SEED_ID_MAX_LEN: usize = 16;

/// Tamanho fixo (em chars alfanuméricos) do C₁ embarcado no campo de transporte.
pub const C1_TRANSPORT_LEN: usize = 11;

/// Tamanho do C₂ apresentado (mesmo formato/tamanho de C₁ para simetria).
pub const C2_TRANSPORT_LEN: usize = 11;

/// Semente compartilhada entre recebedor e banco recebedor.
///
/// Em produção, o material desta semente vive em HSM/Secure Enclave e nunca
/// transita pela memória da aplicação. Aqui a `Seed` opaca prepara essa
/// substituição: o consumidor manipula a referência, não o material.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Seed(pub(crate) [u8; SEED_LEN]);

impl Seed {
    pub fn from_bytes(bytes: [u8; SEED_LEN]) -> Self {
        Self(bytes)
    }

    pub fn try_from_slice(slice: &[u8]) -> Result<Self, McpixError> {
        if slice.len() != SEED_LEN {
            return Err(McpixError::SeedLength {
                expected: SEED_LEN,
                got: slice.len(),
            });
        }
        let mut bytes = [0u8; SEED_LEN];
        bytes.copy_from_slice(slice);
        Ok(Self(bytes))
    }

    /// Acesso ao material da semente. Pública porque persistência (SeedStore)
    /// precisa serializar, e por simetria com `try_from_slice`. A criticidade
    /// é controlada por `ZeroizeOnDrop` e pelo `Debug` redacted.
    pub fn as_bytes(&self) -> &[u8; SEED_LEN] {
        &self.0
    }
}

impl core::fmt::Debug for Seed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Nunca imprime material secreto — apenas confirma presença.
        f.write_str("Seed(REDACTED)")
    }
}

/// Identificador público do recebedor.
///
/// Alfabeto: `[a-zA-Z1-9]` — `'0'` é deliberadamente excluído porque é o caractere
/// de padding do `seed_id_slot` no campo de transporte. Sem essa restrição o
/// parsing posicional teria que carregar um campo de length explícito.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SeedId(String);

impl SeedId {
    pub fn new(value: impl Into<String>) -> Result<Self, McpixError> {
        let value = value.into();
        if value.is_empty() || value.len() > SEED_ID_MAX_LEN {
            return Err(McpixError::SeedIdLength {
                max: SEED_ID_MAX_LEN,
                got: value.len(),
            });
        }
        if !value.bytes().all(|b| b.is_ascii_alphanumeric() && b != b'0') {
            return Err(McpixError::SeedIdCharset);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Código de cobrança (C₁) — derivado de `HMAC(S, T)` e codificado em base32 sem padding.
///
/// Atravessa canais públicos embarcado no campo de transporte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C1(pub(crate) [u8; C1_TRANSPORT_LEN]);

impl C1 {
    pub fn as_str(&self) -> &str {
        // SAFETY: o construtor garante apenas chars alfanuméricos ASCII.
        core::str::from_utf8(&self.0).expect("C1 always alphanumeric ASCII")
    }
}

/// Código de confirmação (C₂) — derivado de `HMAC(S, T || C₁)`, retido localmente
/// até a apresentação do comprovante. Material sensível enquanto vivo.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct C2(pub(crate) [u8; C2_TRANSPORT_LEN]);

impl C2 {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).expect("C2 always alphanumeric ASCII")
    }

    pub fn parse(s: &str) -> Result<Self, McpixError> {
        if s.len() != C2_TRANSPORT_LEN {
            return Err(McpixError::TransportFieldLength(s.len()));
        }
        if !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(McpixError::TransportFieldCharset(0));
        }
        let mut bytes = [0u8; C2_TRANSPORT_LEN];
        bytes.copy_from_slice(s.as_bytes());
        Ok(Self(bytes))
    }
}

impl core::fmt::Debug for C2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("C2(REDACTED)")
    }
}

/// Alias semântico para `C2` quando usado no papel de "código de confirmação apresentado".
pub type ConfirmationCode = C2;

/// Resultado de `generate_charge`: contém o campo de transporte público e referência
/// ao registro retido (consultável depois para validação).
#[derive(Clone, Debug)]
pub struct Charge {
    pub seed_id: SeedId,
    pub counter: u64,
    pub amount_cents: u64,
    pub transport_field: String,
}

/// Registro retido localmente pelo recebedor; armazena C₂ esperado para a transação.
#[derive(Clone)]
pub struct RetainedReceipt {
    pub seed_id: SeedId,
    pub counter: u64,
    pub amount_cents: u64,
    pub expected_c2: C2,
    pub consumed: bool,
}

impl core::fmt::Debug for RetainedReceipt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RetainedReceipt")
            .field("seed_id", &self.seed_id)
            .field("counter", &self.counter)
            .field("amount_cents", &self.amount_cents)
            .field("expected_c2", &self.expected_c2)
            .field("consumed", &self.consumed)
            .finish()
    }
}
