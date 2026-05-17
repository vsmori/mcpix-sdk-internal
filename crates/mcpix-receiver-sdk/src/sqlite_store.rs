//! `SeedStore` em SQLite — implementação opcional via feature `sqlite`.
//!
//! Tabelas:
//! - `seeds(seed_id PRIMARY KEY, material BLOB NOT NULL)`
//! - `receipts(seed_id, counter, amount_cents, expected_c2 BLOB, consumed INTEGER, PRIMARY KEY(seed_id, counter))`
//!
//! O material da semente é armazenado em claro. Em produção isso seria
//! criptografado em repouso pela plataforma (Keystore/Keychain) — a interface
//! `SeedStore` é o ponto de extensão.

use parking_lot::Mutex;
use rusqlite::{params, Connection};

use mcpix_core::error::McpixError;
use mcpix_core::traits::SeedStore;
use mcpix_core::types::{C2, C2_TRANSPORT_LEN, RetainedReceipt, Seed, SeedId};

pub struct SqliteSeedStore {
    conn: Mutex<Connection>,
}

impl SqliteSeedStore {
    pub fn open_in_memory() -> Result<Self, McpixError> {
        let conn = Connection::open_in_memory().map_err(map_err)?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_schema()?;
        Ok(store)
    }

    pub fn open(path: &str) -> Result<Self, McpixError> {
        let conn = Connection::open(path).map_err(map_err)?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), McpixError> {
        self.conn
            .lock()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS seeds (
                     seed_id TEXT PRIMARY KEY,
                     material BLOB NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS receipts (
                     seed_id TEXT NOT NULL,
                     counter INTEGER NOT NULL,
                     amount_cents INTEGER NOT NULL,
                     expected_c2 BLOB NOT NULL,
                     consumed INTEGER NOT NULL DEFAULT 0,
                     PRIMARY KEY (seed_id, counter)
                 );",
            )
            .map_err(map_err)?;
        Ok(())
    }
}

fn map_err(e: rusqlite::Error) -> McpixError {
    McpixError::Storage(e.to_string())
}

impl SeedStore for SqliteSeedStore {
    fn put_seed(&self, seed_id: &SeedId, seed: Seed) -> Result<(), McpixError> {
        let bytes = seed.as_bytes().to_vec();
        self.conn
            .lock()
            .execute(
                "INSERT OR REPLACE INTO seeds (seed_id, material) VALUES (?1, ?2)",
                params![seed_id.as_str(), bytes],
            )
            .map_err(map_err)?;
        Ok(())
    }

    fn get_seed(&self, seed_id: &SeedId) -> Result<Option<Seed>, McpixError> {
        let g = self.conn.lock();
        let mut stmt = g
            .prepare("SELECT material FROM seeds WHERE seed_id = ?1")
            .map_err(map_err)?;
        let mut rows = stmt
            .query(params![seed_id.as_str()])
            .map_err(map_err)?;
        match rows.next().map_err(map_err)? {
            Some(row) => {
                let bytes: Vec<u8> = row.get(0).map_err(map_err)?;
                Ok(Some(Seed::try_from_slice(&bytes)?))
            }
            None => Ok(None),
        }
    }

    fn save_receipt(&self, receipt: RetainedReceipt) -> Result<(), McpixError> {
        let c2_bytes = receipt.expected_c2.as_str().as_bytes();
        self.conn
            .lock()
            .execute(
                "INSERT OR REPLACE INTO receipts (seed_id, counter, amount_cents, expected_c2, consumed) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    receipt.seed_id.as_str(),
                    receipt.counter as i64,
                    receipt.amount_cents as i64,
                    c2_bytes,
                    receipt.consumed as i64,
                ],
            )
            .map_err(map_err)?;
        Ok(())
    }

    fn get_receipt(
        &self,
        seed_id: &SeedId,
        counter: u64,
    ) -> Result<Option<RetainedReceipt>, McpixError> {
        let g = self.conn.lock();
        let mut stmt = g
            .prepare(
                "SELECT amount_cents, expected_c2, consumed FROM receipts WHERE seed_id = ?1 AND counter = ?2",
            )
            .map_err(map_err)?;
        let mut rows = stmt
            .query(params![seed_id.as_str(), counter as i64])
            .map_err(map_err)?;
        match rows.next().map_err(map_err)? {
            Some(row) => {
                let amount_cents: i64 = row.get(0).map_err(map_err)?;
                let c2: Vec<u8> = row.get(1).map_err(map_err)?;
                let consumed: i64 = row.get(2).map_err(map_err)?;
                if c2.len() != C2_TRANSPORT_LEN {
                    return Err(McpixError::Storage("malformed C2 in row".into()));
                }
                let c2 = C2::parse(std::str::from_utf8(&c2).map_err(|e| McpixError::Storage(e.to_string()))?)?;
                Ok(Some(RetainedReceipt {
                    seed_id: seed_id.clone(),
                    counter,
                    amount_cents: amount_cents as u64,
                    expected_c2: c2,
                    consumed: consumed != 0,
                }))
            }
            None => Ok(None),
        }
    }

    fn mark_consumed(&self, seed_id: &SeedId, counter: u64) -> Result<(), McpixError> {
        let n = self
            .conn
            .lock()
            .execute(
                "UPDATE receipts SET consumed = 1 WHERE seed_id = ?1 AND counter = ?2",
                params![seed_id.as_str(), counter as i64],
            )
            .map_err(map_err)?;
        if n == 0 {
            return Err(McpixError::NoRetainedReceipt);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpix_core::state::{apply_generate_charge, GenerateChargeCommand};

    #[test]
    fn sqlite_round_trip_seed_and_receipt() {
        let store = SqliteSeedStore::open_in_memory().unwrap();
        let sid = SeedId::new("R1").unwrap();
        let seed = Seed::from_bytes([0x9F; 32]);
        store.put_seed(&sid, seed.clone()).unwrap();
        let fetched = store.get_seed(&sid).unwrap().unwrap();
        assert_eq!(fetched.as_bytes(), seed.as_bytes());

        let outcome = apply_generate_charge(
            &seed,
            GenerateChargeCommand { seed_id: sid.clone(), counter: 42, amount_cents: 7 },
        );
        store.save_receipt(outcome.retained.clone()).unwrap();
        let got = store.get_receipt(&sid, 42).unwrap().unwrap();
        assert_eq!(got.expected_c2.as_str(), outcome.retained.expected_c2.as_str());
        assert!(!got.consumed);

        store.mark_consumed(&sid, 42).unwrap();
        let got = store.get_receipt(&sid, 42).unwrap().unwrap();
        assert!(got.consumed);
    }
}
