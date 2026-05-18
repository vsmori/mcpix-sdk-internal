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

/// Sela / dessela uma `Seed` com material custodiado em hardware seguro.
///
/// **Modelo de uso.** O `SeedStore` da SDK persiste `Seed` em claro;
/// o threat model (§7.1) marca isso como impl-dependente. Integradores
/// que queiram fechar esse gap envelopam o store com um `SealedSeedStore`
/// (ver `mcpix-receiver-sdk::sealed_store`), que delega o
/// "selo criptográfico" a esta trait. A chave usada para selar **deve
/// viver no hardware seguro**:
///
/// - iOS: chave gerada com `kSecAttrAccessControlPrivateKeyUsage` no
///   Secure Enclave; operações de seal/unseal invocam `SecKeyEncrypt`.
/// - Android: chave `KeyGenParameterSpec` com `setIsStrongBoxBacked(true)`
///   no Keystore; operações via `Cipher.getInstance("AES/GCM/...")`.
/// - Desktop (Linux/Windows/macOS): TPM 2.0 via `tpm2-tss`, derivando a
///   chave de um Endorsement Hierarchy template.
///
/// **Atestation** (opcional) prova ao verificador externo que a chave
/// efetivamente vive no hardware (Apple App Attest, Android Key
/// Attestation chain, TPM Quote). Retornar `Ok(None)` é OK para impls
/// puramente locais que não publicam essa garantia.
///
/// **Por que separar de `SeedStore`.** A camada de persistência
/// (filesystem, SQLite, Keychain) é ortogonal à camada de cripto-selo.
/// Um integrador que use SQLite + iOS Keychain compõe os dois traits;
/// outro que use só TPM + memória usa só o sealer. Manter
/// responsabilidades distintas evita acoplar impls.
pub trait SeedSealer: Send + Sync {
    /// Sela `plain` produzindo um blob opaco que somente este mesmo
    /// sealer (com acesso ao material hw-bound) consegue desselar.
    /// O blob inclui o nonce/IV — chamadas sucessivas com a mesma
    /// `Seed` produzem outputs diferentes (não-determinístico).
    fn seal(&self, plain: &Seed) -> Result<Vec<u8>, McpixError>;

    /// Inverte o seal. Tampering no blob (1 bit flipado) é detectado
    /// pelo AEAD subjacente e retorna erro. Diferença de chave entre
    /// a usada para selar e a usada para desselar idem.
    fn unseal(&self, blob: &[u8]) -> Result<Seed, McpixError>;

    /// Devolve atestação opcional do hardware que custodia a chave.
    /// Default `Ok(None)` para impls que não publicam essa garantia
    /// (mocks, TPM sem CA confiada, etc).
    fn attestation(&self) -> Result<Option<Vec<u8>>, McpixError> {
        Ok(None)
    }
}
