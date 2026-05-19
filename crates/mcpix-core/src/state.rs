//! Núcleo funcional: dada uma trinca `(Seed, Counter T, amount)`, calcula o
//! `Charge` público e o `RetainedReceipt` que o recebedor deve guardar.
//!
//! Esta camada não conhece storage nem rede. As fachadas (`mcpix-receiver-sdk`,
//! `mcpix-bank-payer-mock`) chamam estas funções puras e empurram o resultado
//! para a `SeedStore` injetada.

use crate::crypto::{derive_c2_from_c1, derive_pair, verify_c2};
use crate::error::McpixError;
use crate::transport_field;
use crate::types::{Charge, RetainedReceipt, Seed, SeedId, C1, C2};

/// Comando "gerar nova cobrança": entrada do recebedor.
#[derive(Clone, Debug)]
pub struct GenerateChargeCommand {
    pub seed_id: SeedId,
    pub counter: u64,
    pub amount_cents: u64,
}

/// Saída de `apply_generate_charge`: o que vai para o canal público e o que
/// fica retido localmente. As fachadas persistem o segundo via `SeedStore`.
#[derive(Clone, Debug)]
pub struct ChargeOutcome {
    pub charge: Charge,
    pub retained: RetainedReceipt,
}

pub fn apply_generate_charge(seed: &Seed, cmd: GenerateChargeCommand) -> ChargeOutcome {
    let (c1, c2) = derive_pair(seed, cmd.counter);
    let transport_field = transport_field::encode(&cmd.seed_id, &c1);

    let charge = Charge {
        seed_id: cmd.seed_id.clone(),
        counter: cmd.counter,
        amount_cents: cmd.amount_cents,
        transport_field,
    };

    let retained = RetainedReceipt {
        seed_id: cmd.seed_id,
        counter: cmd.counter,
        amount_cents: cmd.amount_cents,
        expected_c2: c2,
        consumed: false,
    };

    ChargeOutcome { charge, retained }
}

/// Resultado de uma validação local. Não capotamos por C₂ errado — isso é
/// estado válido do protocolo (recebedor pode receber comprovante adulterado).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationOutcome {
    Valid,
    Mismatch,
    Replay,
}

/// Comparação local em tempo constante. Caller deve, ANTES de chamar, ter
/// recuperado `retained` da `SeedStore`. Após `Valid`, é responsabilidade do
/// caller marcar o `RetainedReceipt` como consumido (atomicamente, para evitar
/// race com nova apresentação).
pub fn apply_validate_receipt(retained: &RetainedReceipt, presented: &C2) -> ValidationOutcome {
    if retained.consumed {
        return ValidationOutcome::Replay;
    }
    if verify_c2(&retained.expected_c2, presented) {
        ValidationOutcome::Valid
    } else {
        ValidationOutcome::Mismatch
    }
}

/// Caminho do banco do pagador: a partir do campo público + semente recuperada
/// no banco recebedor, recompõe `C₂` (substituição institucional).
pub fn apply_recover_c2(seed: &Seed, counter: u64, c1: &C1) -> C2 {
    derive_c2_from_c1(seed, counter, c1)
}

/// Saneamento: valida coerência entre o campo recebido (parse) e o `SeedId`
/// esperado. Útil quando o banco do pagador faz dupla checagem após lookup.
pub fn ensure_seed_id_matches(
    field_seed_id: &SeedId,
    lookup_seed_id: &SeedId,
) -> Result<(), McpixError> {
    if field_seed_id == lookup_seed_id {
        Ok(())
    } else {
        Err(McpixError::UnknownSeed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport_field;

    fn seed() -> Seed {
        Seed::from_bytes([0xAB; 32])
    }

    #[test]
    fn generate_then_validate_roundtrip() {
        let cmd = GenerateChargeCommand {
            seed_id: SeedId::new("R1").unwrap(),
            counter: 1,
            amount_cents: 12345,
        };
        let outcome = apply_generate_charge(&seed(), cmd);
        let parsed = transport_field::parse(&outcome.charge.transport_field).unwrap();

        // banco do pagador recompõe C2 a partir do que veio público:
        let c2_recovered = apply_recover_c2(&seed(), outcome.retained.counter, &parsed.c1);
        assert_eq!(
            apply_validate_receipt(&outcome.retained, &c2_recovered),
            ValidationOutcome::Valid
        );
    }

    #[test]
    fn validation_rejects_wrong_c2() {
        let outcome = apply_generate_charge(
            &seed(),
            GenerateChargeCommand {
                seed_id: SeedId::new("R1").unwrap(),
                counter: 1,
                amount_cents: 1,
            },
        );
        let bogus = C2::parse("AAAAAAAAAAA").unwrap();
        assert_eq!(
            apply_validate_receipt(&outcome.retained, &bogus),
            ValidationOutcome::Mismatch
        );
    }

    #[test]
    fn validation_rejects_replay() {
        let mut outcome = apply_generate_charge(
            &seed(),
            GenerateChargeCommand {
                seed_id: SeedId::new("R1").unwrap(),
                counter: 1,
                amount_cents: 1,
            },
        );
        outcome.retained.consumed = true;
        assert_eq!(
            apply_validate_receipt(&outcome.retained, &outcome.retained.expected_c2.clone()),
            ValidationOutcome::Replay
        );
    }
}
