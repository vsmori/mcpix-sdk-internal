//! Simulação do banco do pagador.
//!
//! Caminho de execução:
//! 1. Recebe `instrument_string` do app pagador (string `[a-zA-Z0-9]{26,35}`)
//! 2. Detecta prefixo do protocolo (`PIXOFFv1`) — triagem inicial
//! 3. Faz parse posicional → `(SeedId, C₁)`
//! 4. Consulta semente `S` no banco recebedor (substituição institucional)
//! 5. Recompõe `C₂ = HMAC(S, T || C₁)` localmente
//! 6. Devolve "comprovante" estruturado com `C₂` no campo identificador
//!
//! O ponto crítico do protocolo é o passo 4-5: a derivação determinística
//! permite que o banco do pagador atue como **substituto institucional** do
//! recebedor, sem qualquer canal direto online entre as partes.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

use mcpix_bank_receiver::{BankReceiver, Requester};
use mcpix_core::error::McpixError;
use mcpix_core::state::apply_recover_c2;
use mcpix_core::transport_field::{self, is_protocol_field};
use mcpix_core::types::{C2, SeedId};

/// Comprovante estruturado devolvido pelo banco do pagador ao pagador. Em
/// produção, este é o objeto que viajaria como recibo PIX/SPI. O campo
/// `identifier` é o C₂ recomposto — substitui o C₁ original no transporte de
/// volta para o recebedor.
#[derive(Clone, Debug)]
pub struct PaymentReceipt {
    pub receiver_seed_id: SeedId,
    pub counter_used: u64,
    pub amount_cents: u64,
    pub identifier: String,
    pub note: &'static str,
}

/// Parâmetros do pagamento simulado. O banco do pagador no protocolo real
/// recebe `amount` do app pagador e usa `counter` que veio implícito no campo
/// (aqui passamos explícito — o campo posicional não codifica T; em produção
/// T = timestamp quantizado deriva do header da mensagem).
pub struct PaymentRequest<'a> {
    pub instrument_string: &'a str,
    pub amount_cents: u64,
    /// Contador da transação. Conhecido por ambos os lados pela quantização
    /// do tempo + sequência por seed. Aqui injetado pela camada chamadora.
    pub counter: u64,
    pub requester: Requester,
}

/// Variante com tolerância de janela — usada quando o counter é timestamp
/// quantizado e clocks recebedor/banco podem divergir em até `tolerance`
/// janelas. O banco devolve um C₂ candidato para cada T no intervalo
/// `[counter - tolerance, counter + tolerance]`; o recebedor compara cada
/// candidato com o(s) retained receipt(s) correspondente(s).
///
/// Por que múltiplos candidatos em vez de um T canônico:
/// - O receiver pode ter emitido com T_r e o bank computou T_b a partir do
///   próprio clock; se `|T_r - T_b| <= tolerance`, o receipt válido está em
///   alguma das janelas.
/// - Quem decide a tolerância é política do banco — não do protocolo.
pub struct PaymentRequestWindowed<'a> {
    pub instrument_string: &'a str,
    pub amount_cents: u64,
    /// Quantum atual do banco (`now_unix_secs / window_seconds`).
    pub current_quantum: u64,
    /// Quantos quanta antes/depois aceitar como válidos. Tipicamente 1 (=±30s
    /// com janela de 30s).
    pub tolerance_windows: u32,
    pub requester: Requester,
}

/// Saída da variante com tolerância: comprovante "tentativa" para cada T no
/// intervalo. O recebedor tenta validar cada um — o primeiro que bater no
/// retained receipt é o efetivo.
#[derive(Clone, Debug)]
pub struct WindowedPaymentReceipt {
    pub receiver_seed_id: SeedId,
    pub amount_cents: u64,
    /// Candidatos `(counter, c2_identifier)` em ordem crescente de T.
    pub candidates: Vec<(u64, String)>,
    pub note: &'static str,
}

pub struct PayerBankMock<'a> {
    bank_receiver: &'a dyn BankReceiver,
}

impl<'a> PayerBankMock<'a> {
    pub fn new(bank_receiver: &'a dyn BankReceiver) -> Self {
        Self { bank_receiver }
    }

    pub fn process_payment(
        &self,
        req: PaymentRequest<'_>,
    ) -> Result<PaymentReceipt, McpixError> {
        if !is_protocol_field(req.instrument_string) {
            return Err(McpixError::TransportFieldPrefix);
        }
        let parsed = transport_field::parse(req.instrument_string)?;
        let seed = self
            .bank_receiver
            .lookup_seed(&parsed.seed_id, &req.requester)?;
        // Substituição institucional: o banco do pagador reconstrói C₂ a partir
        // de (S, T, C₁). Mesma função, mesmos argumentos → mesmo resultado que
        // o recebedor produziu offline. Daí o nome do esquema.
        let c2: C2 = apply_recover_c2(&seed, req.counter, &parsed.c1);
        Ok(PaymentReceipt {
            receiver_seed_id: parsed.seed_id,
            counter_used: req.counter,
            amount_cents: req.amount_cents,
            identifier: c2.as_str().to_string(),
            note: "settled via institutional substitution",
        })
    }

    /// Processa pagamento gerando candidatos para ±`tolerance_windows`
    /// janelas em torno de `current_quantum`. Usado quando T = timestamp
    /// quantizado e há drift potencial entre relógios.
    pub fn process_payment_windowed(
        &self,
        req: PaymentRequestWindowed<'_>,
    ) -> Result<WindowedPaymentReceipt, McpixError> {
        if !is_protocol_field(req.instrument_string) {
            return Err(McpixError::TransportFieldPrefix);
        }
        let parsed = transport_field::parse(req.instrument_string)?;
        let seed = self
            .bank_receiver
            .lookup_seed(&parsed.seed_id, &req.requester)?;

        let tol = req.tolerance_windows as u64;
        let lo = req.current_quantum.saturating_sub(tol);
        let hi = req.current_quantum.saturating_add(tol);
        let mut candidates = Vec::with_capacity((hi - lo + 1) as usize);
        for t in lo..=hi {
            let c2: C2 = apply_recover_c2(&seed, t, &parsed.c1);
            candidates.push((t, c2.as_str().to_string()));
        }

        Ok(WindowedPaymentReceipt {
            receiver_seed_id: parsed.seed_id,
            amount_cents: req.amount_cents,
            candidates,
            note: "settled via institutional substitution (windowed)",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpix_bank_receiver::InMemoryBankReceiver;
    use mcpix_core::state::{apply_generate_charge, GenerateChargeCommand};
    use mcpix_core::types::Seed;

    #[test]
    fn payer_bank_recovers_same_c2_as_receiver() {
        let seed = Seed::from_bytes([0x33; 32]);
        let sid = SeedId::new("R1").unwrap();
        let bank_r = InMemoryBankReceiver::new();
        bank_r.register_seed(&sid, seed.clone()).unwrap();

        let outcome = apply_generate_charge(
            &seed,
            GenerateChargeCommand { seed_id: sid.clone(), counter: 5, amount_cents: 1000 },
        );

        let payer_bank = PayerBankMock::new(&bank_r);
        let receipt = payer_bank
            .process_payment(PaymentRequest {
                instrument_string: &outcome.charge.transport_field,
                amount_cents: 1000,
                counter: 5,
                requester: Requester { institution_id: "PAYER".into() },
            })
            .unwrap();

        assert_eq!(receipt.identifier, outcome.retained.expected_c2.as_str());
    }

    #[test]
    fn rejects_foreign_instrument() {
        let bank_r = InMemoryBankReceiver::new();
        let payer_bank = PayerBankMock::new(&bank_r);
        let err = payer_bank
            .process_payment(PaymentRequest {
                instrument_string: "OTHERSCHEMA000000000000000000000000",
                amount_cents: 1,
                counter: 1,
                requester: Requester { institution_id: "PAYER".into() },
            })
            .unwrap_err();
        assert_eq!(err, McpixError::TransportFieldPrefix);
    }

    #[test]
    fn windowed_produces_2n_plus_1_candidates_with_correct_c2_at_center() {
        let seed = Seed::from_bytes([0x55; 32]);
        let sid = SeedId::new("R1").unwrap();
        let bank_r = InMemoryBankReceiver::new();
        bank_r.register_seed(&sid, seed.clone()).unwrap();

        // Recebedor emitiu com T=100 (quantum específico).
        let outcome = apply_generate_charge(
            &seed,
            GenerateChargeCommand { seed_id: sid.clone(), counter: 100, amount_cents: 50 },
        );

        let payer_bank = PayerBankMock::new(&bank_r);
        // Banco computou quantum 100 (clocks alinhados) e aceita ±1.
        let receipt = payer_bank
            .process_payment_windowed(PaymentRequestWindowed {
                instrument_string: &outcome.charge.transport_field,
                amount_cents: 50,
                current_quantum: 100,
                tolerance_windows: 1,
                requester: Requester { institution_id: "PAYER".into() },
            })
            .unwrap();

        assert_eq!(receipt.candidates.len(), 3); // 99, 100, 101
        let (t_center, c2_center) = &receipt.candidates[1];
        assert_eq!(*t_center, 100);
        assert_eq!(c2_center, outcome.retained.expected_c2.as_str());
    }

    #[test]
    fn windowed_with_drifted_clock_still_includes_correct_quantum() {
        let seed = Seed::from_bytes([0x55; 32]);
        let sid = SeedId::new("R1").unwrap();
        let bank_r = InMemoryBankReceiver::new();
        bank_r.register_seed(&sid, seed.clone()).unwrap();

        // Recebedor emitiu com T=100; banco está atrasado 1 quantum (T=99).
        let outcome = apply_generate_charge(
            &seed,
            GenerateChargeCommand { seed_id: sid.clone(), counter: 100, amount_cents: 50 },
        );
        let payer_bank = PayerBankMock::new(&bank_r);
        let receipt = payer_bank
            .process_payment_windowed(PaymentRequestWindowed {
                instrument_string: &outcome.charge.transport_field,
                amount_cents: 50,
                current_quantum: 99,
                tolerance_windows: 1,
                requester: Requester { institution_id: "PAYER".into() },
            })
            .unwrap();

        // Candidatos: 98, 99, 100. O correto (100) está incluído.
        let matched = receipt
            .candidates
            .iter()
            .any(|(t, c2)| *t == 100 && c2 == outcome.retained.expected_c2.as_str());
        assert!(matched, "candidate set should include T=100 within ±1 window");
    }

    #[test]
    fn windowed_excludes_drift_beyond_tolerance() {
        let seed = Seed::from_bytes([0x55; 32]);
        let sid = SeedId::new("R1").unwrap();
        let bank_r = InMemoryBankReceiver::new();
        bank_r.register_seed(&sid, seed.clone()).unwrap();

        let outcome = apply_generate_charge(
            &seed,
            GenerateChargeCommand { seed_id: sid.clone(), counter: 100, amount_cents: 50 },
        );
        let payer_bank = PayerBankMock::new(&bank_r);
        // Banco com clock 5 quanta adiantado, tolerância apenas ±1.
        let receipt = payer_bank
            .process_payment_windowed(PaymentRequestWindowed {
                instrument_string: &outcome.charge.transport_field,
                amount_cents: 50,
                current_quantum: 105,
                tolerance_windows: 1,
                requester: Requester { institution_id: "PAYER".into() },
            })
            .unwrap();

        let any_match = receipt
            .candidates
            .iter()
            .any(|(_, c2)| c2 == outcome.retained.expected_c2.as_str());
        assert!(!any_match, "drift beyond tolerance must not be acceptable");
    }
}
