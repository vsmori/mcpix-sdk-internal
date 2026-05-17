//! Contratos para os efeitos colaterais que o núcleo precisa.
//!
//! O núcleo nunca instancia clientes HTTP, abre arquivos ou fala com hardware
//! seguro diretamente. As plataformas hospedeiras (Swift/Kotlin/.NET) — ou os
//! crates de fachada (`mcpix-receiver-sdk`, etc.) — implementam estas traits
//! e injetam suas implementações.
//!
//! Por que `Send + Sync`: bindings idiomáticos em Kotlin Coroutines e
//! Swift `async/await` cruzam threads; o núcleo precisa ser usável a partir
//! de qualquer pool sem locks externos.

use crate::error::McpixError;
use crate::types::{RetainedReceipt, Seed, SeedId};

/// Persistência das sementes (lado recebedor) e dos C₂ retidos por transação.
///
/// A interface assume operações atômicas por chave. Implementações que usem
/// banco de dados local devem cobrir isso via transação; a implementação
/// in-memory usa `Mutex`.
pub trait SeedStore: Send + Sync {
    fn put_seed(&self, seed_id: &SeedId, seed: Seed) -> Result<(), McpixError>;
    fn get_seed(&self, seed_id: &SeedId) -> Result<Option<Seed>, McpixError>;

    fn save_receipt(&self, receipt: RetainedReceipt) -> Result<(), McpixError>;
    fn get_receipt(
        &self,
        seed_id: &SeedId,
        counter: u64,
    ) -> Result<Option<RetainedReceipt>, McpixError>;
    fn mark_consumed(&self, seed_id: &SeedId, counter: u64) -> Result<(), McpixError>;
}

/// Contador monotônico por `SeedId`. Em produção, vive em HSM/Secure Enclave
/// para impedir reuso por rollback. Aqui a interface basta para a substituição
/// futura sem mudar o núcleo.
pub trait Counter: Send + Sync {
    /// Reserva e retorna o próximo valor de contador para `seed_id`.
    /// Após `next()` ter retornado `n`, qualquer chamada subsequente deve
    /// retornar `> n` (estritamente crescente).
    fn next(&self, seed_id: &SeedId) -> Result<u64, McpixError>;
}

/// Geração de bytes aleatórios criptograficamente fortes. Separar via trait
/// permite testes determinísticos sem misturar uma RNG fake na superfície
/// pública das fachadas.
pub trait SecureRandom: Send + Sync {
    fn fill(&self, out: &mut [u8]) -> Result<(), McpixError>;
}

/// Relógio injetável. O núcleo não consulta o relógio do sistema por conta
/// própria — sempre via esta trait — para que testes possam controlar tempo
/// e para que a parametrização "T = timestamp quantizado" seja possível na
/// próxima versão sem mexer em `crypto`.
pub trait Clock: Send + Sync {
    fn now_unix_secs(&self) -> u64;
}

/// Transporte de baixo nível para integrações inter-institucionais.
/// O núcleo formata o pedido, a fachada nativa (Swift/Kotlin/.NET) entrega.
///
/// Mantemos o tipo o mais opaco possível: bytes de requisição e bytes de
/// resposta, sem assumir HTTP semântico no núcleo.
pub trait HttpTransport: Send + Sync {
    fn send_request(&self, request: RawRequest) -> Result<RawResponse, McpixError>;
}

#[derive(Clone, Debug)]
pub struct RawRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
