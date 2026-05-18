# Secure Element — pattern de integração

Este documento mostra como fechar o gap **§7.1 do `THREAT_MODEL.md`**
("vazamento de `SeedStore` local") plugando uma fonte de chave em
hardware seguro no `SeedSealer` da SDK.

## O contrato

```rust
pub trait SeedSealer: Send + Sync {
    fn seal(&self, plain: &Seed) -> Result<Vec<u8>, McpixError>;
    fn unseal(&self, blob: &[u8]) -> Result<Seed, McpixError>;
    fn attestation(&self) -> Result<Option<Vec<u8>>, McpixError> { Ok(None) }
}
```

Definido em `mcpix-core::traits::SeedSealer`. A SDK provê
[`ChaChaSealer`](../crates/mcpix-receiver-sdk/src/sealed_store.rs)
como **mock de referência** com chave em RAM — ponto de partida
para você ver o pattern funcionar e a estrutura do blob (60 bytes:
nonce 12 + ciphertext 32 + tag 16).

A meta da integração real é simples: a `key` que entra em
`ChaChaSealer::new(key)` deixa de viver na heap do processo e passa
a ser **referência opaca** para uma chave que existe **dentro** do
Secure Enclave / StrongBox / TPM, com as operações
seal/unseal sendo chamadas para o hardware.

## Composição no SDK

```rust
use std::sync::Arc;
use mcpix_receiver_sdk::{
    ReceiverSdk, monotonic_counter::InMemoryCounter,
    system_random::OsRandom,
    sealed_store::{ChaChaSealer, SealedInMemorySeedStore},
};

// 1) Conseguir o material hw-bound. No mock, é estático:
let sealer = ChaChaSealer::new([0x77; 32]);
// Em produção: SecureEnclaveSealer::open("com.your.bank.seed_key")?

// 2) Wrap como SeedStore. Aqui em memória; troque por persistência
//    real (SQLite, Keychain item, EEPROM) reimplementando a trait.
let store = Arc::new(SealedInMemorySeedStore::new(sealer));

// 3) Plug em ReceiverSdk sem mudar nenhuma linha do app.
let sdk = ReceiverSdk::new(store, Arc::new(InMemoryCounter::new()), Arc::new(OsRandom));
```

A `Seed` em claro **nunca** entra no `HashMap` interno. O teste
`sealed_blob_does_not_contain_plaintext_seed` verifica esta
invariante em CI a cada push.

## Pattern: iOS (Secure Enclave)

A Secure Enclave do iOS (chip Apple Silicon, A7+) custodia chaves
EC P-256 que **não saem** do silício. As operações cripto rodam
dentro do enclave; o app só vê handles e outputs.

```swift
// Swift side — gera chave persistente uma vez por install
import CryptoKit
import Security

let access = SecAccessControlCreateWithFlags(
    nil,
    kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
    [.privateKeyUsage, .biometryCurrentSet],  // exige Face/Touch ID
    nil
)!

let attributes: [String: Any] = [
    kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
    kSecAttrKeySizeInBits as String: 256,
    kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
    kSecAttrLabel as String: "com.your.bank.mcpix.seed_key",
    kSecPrivateKeyAttrs as String: [
        kSecAttrIsPermanent as String: true,
        kSecAttrAccessControl as String: access,
    ],
]

var error: Unmanaged<CFError>?
let privateKey = SecKeyCreateRandomKey(attributes as CFDictionary, &error)!
```

```rust
// Rust side — impl plugando via FFI (ou via uniffi callback interface)
pub struct SecureEnclaveSealer {
    /// Handle Swift do `SecKey` na Secure Enclave. Opaco para o Rust.
    pub key_handle: SwiftCallbackHandle,
}

impl SeedSealer for SecureEnclaveSealer {
    fn seal(&self, plain: &Seed) -> Result<Vec<u8>, McpixError> {
        // Atravessa para Swift via callback interface:
        //   SecKeyCreateEncryptedData(key, .eciesEncryptionCofactorVariableIVX963SHA256AESGCM, data)
        // Retorno: blob ECIES + AES-GCM. Tamanho ~120 bytes.
        self.key_handle.encrypt(plain.as_bytes())
            .map_err(|e| McpixError::Storage(format!("se seal: {e}")))
    }

    fn unseal(&self, blob: &[u8]) -> Result<Seed, McpixError> {
        let pt = self.key_handle.decrypt(blob)
            .map_err(|e| McpixError::Storage(format!("se unseal: {e}")))?;
        // Validações + Seed::try_from_slice(&pt)
        ...
    }

    fn attestation(&self) -> Result<Option<Vec<u8>>, McpixError> {
        // App Attest (DeviceCheck) emite asserção assinada pela Apple
        // CA confirmando que o app integral pediu a operação.
        Ok(Some(self.key_handle.app_attest_assertion()?))
    }
}
```

**Ganho**: roubo do dispositivo desbloqueado é o **único** vetor que
preserva acesso à Seed. Backup local, jailbreak passivo, dump de
RAM via debugger — todos falham porque a chave nunca está em
user-space.

## Pattern: Android (StrongBox / TEE)

Android Keystore com `setIsStrongBoxBacked(true)` armazena a chave
no chip Titan M2 (Pixel 6+) ou TEE equivalente em outros OEMs.

```kotlin
val spec = KeyGenParameterSpec.Builder("mcpix.seed_key",
    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
    .setKeySize(256)
    .setIsStrongBoxBacked(true)
    .setUserAuthenticationRequired(true)
    .setUserAuthenticationParameters(60, KeyProperties.AUTH_BIOMETRIC_STRONG)
    .setAttestationChallenge("mcpix-receiver-v1".toByteArray())
    .build()

val keyGen = KeyGenerator.getInstance("AES", "AndroidKeyStore")
keyGen.init(spec)
keyGen.generateKey()
```

```rust
// Lado Rust idêntico em estrutura ao iOS; só muda a callback interface
pub struct StrongBoxSealer { /* JNI handle */ }
impl SeedSealer for StrongBoxSealer { /* … */ }
```

**Atestação**: o `Cert::getAttestation()` da chave devolve uma cadeia
X.509 assinada pela CA Google, comprovando que a chave vive em
StrongBox. Use em `SeedSealer::attestation()` para o banco verificar
a posse antes de autorizar transações de alto valor.

## Pattern: TPM 2.0 (desktop/servidor)

Para .NET backend (banco recebedor rodando em servidor Windows ou
appliance Linux), a chave vive no TPM 2.0 do host.

```rust
// Via tpm2-tss (FFI) ou tpm-i-rust (Rust nativo)
pub struct TpmSealer {
    ctx: tpm2_tss::Context,
    /// Handle persistente. Tipicamente 0x81000001 ou similar; criado
    /// na primeira execução via tpm2_createprimary + tpm2_evictcontrol.
    handle: tpm2_tss::PersistentHandle,
}

impl SeedSealer for TpmSealer {
    fn seal(&self, plain: &Seed) -> Result<Vec<u8>, McpixError> {
        // tpm2_seal usa o TPM primary key da Storage Hierarchy.
        // Output: blob com priv + pub portions, ~300 bytes.
        self.ctx.seal(plain.as_bytes(), self.handle)
            .map_err(|e| McpixError::Storage(format!("tpm seal: {e}")))
    }
    fn unseal(&self, blob: &[u8]) -> Result<Seed, McpixError> { /* … */ }

    fn attestation(&self) -> Result<Option<Vec<u8>>, McpixError> {
        // TPM Quote — assinatura da chave EK sobre o PCR state.
        // Verificável contra CA do fabricante.
        Ok(Some(self.ctx.quote(self.handle)?))
    }
}
```

## Testes ancorados pelo SDK

Mesmo sem hw-bound real, o SDK ancora invariantes que valem **para
qualquer impl** correta:

- `sealed_blob_does_not_contain_plaintext_seed` — blob persistido
  nunca pode conter os 32 bytes da Seed em sequência. Se a sua
  impl de `SeedSealer` falhar isto, ela está quebrada — não
  importa quão hw-bound for a chave.
- `seal_is_not_deterministic` — duas chamadas de `seal` sobre a
  mesma Seed devem produzir blobs diferentes. Sem nonce aleatório,
  dois slots com mesma Seed seriam distinguíveis.
- `tampered_blob_fails_decrypt` — AEAD rejeita 1-bit flip.
- `wrong_sealer_fails_unseal` — material de outro dispositivo
  falha unseal em vez de retornar lixo.
- `full_charge_validation_through_sealed_store` — `ReceiverSdk`
  inteiro funciona com o sealed store no lugar do `InMemorySeedStore`.

Use estes testes como suite de aceitação para sua impl real:
substitua `ChaChaSealer::new([k; 32])` por `SecureEnclaveSealer::new(...)`
ou similar, rode o módulo de testes, todos devem continuar verdes.

## Status atual da SDK

| Componente | Status |
|---|---|
| Trait `SeedSealer` | ✅ pronto (`mcpix-core::traits`) |
| `SealedInMemorySeedStore` | ✅ pronto (`mcpix-receiver-sdk::sealed_store`) |
| Mock `ChaChaSealer` | ✅ pronto, **não use em produção** |
| Impl iOS Secure Enclave | ⏳ esqueleto neste doc — exige device de validação |
| Impl Android StrongBox | ⏳ idem |
| Impl TPM 2.0 | ⏳ idem |
| Atestação verificada pelo banco | ⏳ exige PKI da plataforma + endpoint |

A camada Rust + contrato + suite de testes está pronta. O que falta
é fechar o último kilômetro em cada plataforma — trabalho que precisa
de hardware real para validar.
