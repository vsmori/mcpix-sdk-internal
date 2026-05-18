//! Bindings WebAssembly do mcpix-sdk.
//!
//! Expõe um `WasmDemo` que mantém em memória dois "bancos" simulados:
//! - **Recebedor**: custodia a `Seed`, gera cobranças, valida comprovantes.
//! - **Pagador**: parseia o campo de transporte público, "consulta" a
//!   semente do recebedor (em produção via mTLS — aqui acesso direto à
//!   mesma memória da demo) e reconstrói C₂.
//!
//! Esse design coloca os dois lados do protocolo dentro de um único
//! módulo wasm consumido pela UI em `examples/web-demo/`. Para a UI
//! tudo é JS plain — `WasmDemo` retorna structs serializáveis.
//!
//! ## Por que não importar `mcpix-bank-payer-mock`
//!
//! Aquele crate depende de `mcpix-bank-receiver`, que puxa axum + tokio +
//! http stack — incompatível com wasm32-unknown-unknown. Aqui usamos
//! `apply_recover_c2` diretamente, replicando a lógica de uma linha.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use wasm_bindgen::prelude::*;

use mcpix_core::state::{apply_recover_c2, ValidationOutcome};
use mcpix_core::traits::{Clock, SeedStore};
use mcpix_core::transport_field;
use mcpix_core::types::SeedId;
use mcpix_receiver_sdk::memory_store::InMemorySeedStore;
use mcpix_receiver_sdk::monotonic_counter::InMemoryCounter;
use mcpix_receiver_sdk::system_random::OsRandom;
use mcpix_receiver_sdk::timestamp_counter::TimestampQuantizedCounter;
use mcpix_receiver_sdk::ReceiverSdk;

// ─────────────────────────────────────────────────────────────────────────
// JsInjectableClock — Clock cujo tempo é controlado por chamadas a
// `tick(now_secs)` do JS. Permite usar `TimestampQuantizedCounter` em
// wasm sem depender de `std::time::SystemTime` (que não é confiável em
// wasm32-unknown-unknown sem deps adicionais).
// ─────────────────────────────────────────────────────────────────────────

struct JsInjectableClock {
    now_secs: AtomicU64,
}

impl Clock for JsInjectableClock {
    fn now_unix_secs(&self) -> u64 {
        self.now_secs.load(Ordering::Relaxed)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Panic hook — encaminha pânicos do Rust para `console.error` para que
// crashes não fiquem silenciosos no JS.
// ─────────────────────────────────────────────────────────────────────────

#[wasm_bindgen(start)]
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("[mcpix-wasm panic] {info}");
        web_error(&msg);
    }));
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn web_error(s: &str);
}

// ─────────────────────────────────────────────────────────────────────────
// API pública
// ─────────────────────────────────────────────────────────────────────────

/// Estado completo da demo — segura ambos os "bancos". Wrap em
/// `wasm_bindgen` opaco; JS só tem o handle, não acessa campos.
#[wasm_bindgen]
pub struct WasmDemo {
    store: Arc<InMemorySeedStore>,
    sdk: ReceiverSdk,
    /// `Some` apenas em modo quantizado — handle para o clock
    /// injetável atualizado via `tick`.
    clock: Option<Arc<JsInjectableClock>>,
    /// `Some` apenas em modo quantizado — usado para `current_quantum`.
    window_seconds: Option<u64>,
}

#[wasm_bindgen]
impl WasmDemo {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmDemo {
        let store = Arc::new(InMemorySeedStore::new());
        let counter = Arc::new(InMemoryCounter::new());
        let rng = Arc::new(OsRandom);
        let sdk = ReceiverSdk::new(store.clone(), counter, rng);
        WasmDemo {
            store,
            sdk,
            clock: None,
            window_seconds: None,
        }
    }

    /// Cria uma instância em **modo timestamp quantizado**. O contador
    /// `T` deriva de `now_unix_secs / window_seconds`. Re-emissão
    /// dentro do mesmo quantum falha com `CounterCollision`.
    ///
    /// O clock vem do JS via [`Self::tick`]; chame antes de cada
    /// operação. `initial_time_secs` evita um primeiro tick com
    /// `now=0` (quantum 0 colidiria com qualquer T futuro).
    pub fn new_quantized(window_seconds: u64, initial_time_secs: u64) -> WasmDemo {
        let store = Arc::new(InMemorySeedStore::new());
        let clock = Arc::new(JsInjectableClock {
            now_secs: AtomicU64::new(initial_time_secs),
        });
        let counter = Arc::new(TimestampQuantizedCounter::with_window(
            clock.clone(),
            window_seconds,
        ));
        let rng = Arc::new(OsRandom);
        let sdk = ReceiverSdk::new(store.clone(), counter, rng);
        WasmDemo {
            store,
            sdk,
            clock: Some(clock),
            window_seconds: Some(window_seconds),
        }
    }

    /// Em modo quantizado: atualiza o relógio injetado para `now_secs`.
    /// No-op em modo sequencial.
    pub fn tick(&self, now_secs: u64) {
        if let Some(c) = &self.clock {
            c.now_secs.store(now_secs, Ordering::Relaxed);
        }
    }

    /// Em modo quantizado: devolve o `T` atual = `now / window_seconds`.
    /// `None` em modo sequencial (contador é opaco, não derivado do
    /// tempo).
    pub fn current_quantum(&self) -> Option<u64> {
        let ws = self.window_seconds?;
        let clock = self.clock.as_ref()?;
        Some(clock.now_unix_secs() / ws)
    }

    /// Window seconds configurado (None em modo sequencial).
    pub fn window_seconds(&self) -> Option<u64> {
        self.window_seconds
    }

    /// Registra um recebedor (gera Seed aleatória e armazena sob `seed_id`).
    pub fn register(&self, seed_id: &str) -> Result<JsValue, JsValue> {
        let sid = SeedId::new(seed_id).map_err(stringify)?;
        let proof = self.sdk.register(sid).map_err(stringify)?;
        let out = RegisterOut {
            seed_id: proof.seed_id.as_str().to_string(),
        };
        serde_to_js(&out)
    }

    /// Gera uma cobrança nova: counter T++, deriva C₁ e C₂, salva C₂
    /// localmente, devolve o campo de transporte público (35 chars).
    pub fn generate_charge(&self, seed_id: &str, amount_cents: u64) -> Result<JsValue, JsValue> {
        let sid = SeedId::new(seed_id).map_err(stringify)?;
        let charge = self.sdk.generate_charge(&sid, amount_cents).map_err(stringify)?;
        // C₂ é segredo no fluxo real — só expomos aqui porque a demo precisa
        // mostrar visualmente "isto fica retido no recebedor".
        let retained = self
            .sdk
            .peek_retained(&sid, charge.counter)
            .map_err(stringify)?
            .ok_or_else(|| JsValue::from_str("retained receipt missing"))?;
        let out = GenerateChargeOut {
            seed_id: charge.seed_id.as_str().to_string(),
            counter: charge.counter,
            amount_cents: charge.amount_cents,
            transport_field: charge.transport_field,
            // C₂ exposto APENAS pela UI da demo para mostrar o retido.
            // Não é exposto no protocolo real — vive só no recebedor.
            retained_c2: retained.expected_c2.as_str().to_string(),
        };
        serde_to_js(&out)
    }

    /// Simula o lado pagador: parseia o campo de transporte, "consulta"
    /// a semente do banco recebedor (acesso direto em memória — em
    /// produção é mTLS), e reconstrói C₂ via `apply_recover_c2`.
    ///
    /// O `counter` precisa vir do contexto (timestamp quantizado real,
    /// ou injeção explícita como aqui). A demo já carrega `counter` da
    /// cobrança recém-gerada.
    pub fn payer_recover_c2(&self, transport_field: &str, counter: u64) -> Result<JsValue, JsValue> {
        let parsed = transport_field::parse(transport_field).map_err(stringify)?;
        // Acesso direto ao seed store — equivalente lógico ao
        // `BankReceiver::lookup_seed` que em produção atravessa mTLS.
        let seed = self
            .store
            .get_seed(&parsed.seed_id)
            .map_err(stringify)?
            .ok_or_else(|| JsValue::from_str("unknown seed (não registrado neste banco)"))?;
        let c2 = apply_recover_c2(&seed, counter, &parsed.c1);
        let out = PayerRecoverOut {
            seed_id: parsed.seed_id.as_str().to_string(),
            c1: parsed.c1.as_str().to_string(),
            recovered_c2: c2.as_str().to_string(),
            counter,
        };
        serde_to_js(&out)
    }

    /// Recebedor valida um C₂ apresentado pelo pagador. Marca como
    /// consumido se válido (defesa de replay).
    pub fn validate_receipt(
        &self,
        seed_id: &str,
        counter: u64,
        presented_c2: &str,
    ) -> Result<String, JsValue> {
        let sid = SeedId::new(seed_id).map_err(stringify)?;
        let outcome = self
            .sdk
            .validate_receipt(&sid, counter, presented_c2)
            .map_err(stringify)?;
        Ok(match outcome {
            ValidationOutcome::Valid => "Valid",
            ValidationOutcome::Mismatch => "Mismatch",
            ValidationOutcome::Replay => "Replay",
        }
        .to_string())
    }
}

impl Default for WasmDemo {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Tipos de saída para o JS. wasm-bindgen serializa via `serde-wasm-bindgen`
// que está disponível em `wasm-bindgen` desde 0.2 sem dep extra para
// structs simples — usamos `JsValue::from_serde` equivalente via Reflect.
// Para evitar adicionar `serde-wasm-bindgen`, montamos objetos JS via
// `js_sys::Object` setando campos string/number. É mais código mas zero
// deps extras.
// ─────────────────────────────────────────────────────────────────────────

struct RegisterOut {
    seed_id: String,
}

struct GenerateChargeOut {
    seed_id: String,
    counter: u64,
    amount_cents: u64,
    transport_field: String,
    retained_c2: String,
}

struct PayerRecoverOut {
    seed_id: String,
    c1: String,
    recovered_c2: String,
    counter: u64,
}

trait ToJsObj {
    fn to_js(&self) -> JsValue;
}

impl ToJsObj for RegisterOut {
    fn to_js(&self) -> JsValue {
        let s = format!(r#"{{"seed_id":"{}"}}"#, esc(&self.seed_id));
        parse_json(&s)
    }
}

impl ToJsObj for GenerateChargeOut {
    fn to_js(&self) -> JsValue {
        let s = format!(
            r#"{{"seed_id":"{}","counter":{},"amount_cents":{},"transport_field":"{}","retained_c2":"{}"}}"#,
            esc(&self.seed_id),
            self.counter,
            self.amount_cents,
            esc(&self.transport_field),
            esc(&self.retained_c2)
        );
        parse_json(&s)
    }
}

impl ToJsObj for PayerRecoverOut {
    fn to_js(&self) -> JsValue {
        let s = format!(
            r#"{{"seed_id":"{}","c1":"{}","recovered_c2":"{}","counter":{}}}"#,
            esc(&self.seed_id),
            esc(&self.c1),
            esc(&self.recovered_c2),
            self.counter
        );
        parse_json(&s)
    }
}

fn serde_to_js<T: ToJsObj>(v: &T) -> Result<JsValue, JsValue> {
    Ok(v.to_js())
}

fn esc(s: &str) -> String {
    // Os campos da SDK são ASCII alfanuméricos (SeedId, C1, C2, transport
    // field). Não há caracteres especiais a escapar — uma checagem barata
    // garante essa invariante e evita injection caso o contrato mude.
    debug_assert!(s.bytes().all(|b| b.is_ascii_alphanumeric()));
    s.to_string()
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = JSON, js_name = parse)]
    fn js_json_parse(s: &str) -> JsValue;
}

fn parse_json(s: &str) -> JsValue {
    js_json_parse(s)
}

fn stringify<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}
