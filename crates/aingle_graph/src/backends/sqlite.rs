//! SQLite storage backend
//!
//! Provides portable, lightweight storage for IoT and embedded devices.
//! SQLite is ideal for resource-constrained environments.

use super::StorageBackend;
use crate::{Error, Result, Triple, TripleId};
use rusqlite::{params, Connection};
use std::sync::Mutex;

/// SQLite-based storage backend
pub struct SqliteBackend {
    /// Database connection
    conn: Mutex<Connection>,
}

impl SqliteBackend {
    /// Open or create a SQLite database at the given path
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| Error::Storage(format!("failed to open sqlite db: {}", e)))?;

        let backend = Self {
            conn: Mutex::new(conn),
        };

        backend.init_schema()?;
        Ok(backend)
    }

    /// Open an in-memory SQLite database
    pub fn memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Storage(format!("failed to create memory db: {}", e)))?;

        let backend = Self {
            conn: Mutex::new(conn),
        };

        backend.init_schema()?;
        Ok(backend)
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS triples (
                id BLOB PRIMARY KEY,
                data BLOB NOT NULL
            )",
            [],
        )
        .map_err(|e| Error::Storage(format!("failed to create table: {}", e)))?;

        // Create index for faster lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_triple_id ON triples(id)",
            [],
        )
        .map_err(|e| Error::Storage(format!("failed to create index: {}", e)))?;

        Ok(())
    }
}

impl StorageBackend for SqliteBackend {
    fn put(&self, id: &TripleId, triple: &Triple) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let bytes = triple.to_bytes();
        conn.execute(
            "INSERT OR REPLACE INTO triples (id, data) VALUES (?1, ?2)",
            params![id.as_bytes().as_slice(), bytes],
        )
        .map_err(|e| Error::Storage(format!("sqlite insert error: {}", e)))?;

        Ok(())
    }

    fn get(&self, id: &TripleId) -> Result<Option<Triple>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let mut stmt = conn
            .prepare("SELECT data FROM triples WHERE id = ?1")
            .map_err(|e| Error::Storage(format!("sqlite prepare error: {}", e)))?;

        let result: std::result::Result<Vec<u8>, _> =
            stmt.query_row(params![id.as_bytes().as_slice()], |row| row.get(0));

        match result {
            Ok(bytes) => Ok(Triple::from_bytes(&bytes)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Storage(format!("sqlite query error: {}", e))),
        }
    }

    fn delete(&self, id: &TripleId) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let changes = conn
            .execute(
                "DELETE FROM triples WHERE id = ?1",
                params![id.as_bytes().as_slice()],
            )
            .map_err(|e| Error::Storage(format!("sqlite delete error: {}", e)))?;

        Ok(changes > 0)
    }

    fn iter_all(&self) -> Result<Vec<Triple>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let mut stmt = conn
            .prepare("SELECT data FROM triples")
            .map_err(|e| Error::Storage(format!("sqlite prepare error: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(bytes)
            })
            .map_err(|e| Error::Storage(format!("sqlite query error: {}", e)))?;

        let mut triples = Vec::new();
        for row in rows {
            if let Ok(bytes) = row {
                if let Some(triple) = Triple::from_bytes(&bytes) {
                    triples.push(triple);
                }
            }
        }

        Ok(triples)
    }

    fn count(&self) -> usize {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        conn.query_row("SELECT COUNT(*) FROM triples", [], |row| row.get(0))
            .unwrap_or(0)
    }

    fn size_bytes(&self) -> usize {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        // Get page count and page size
        let page_count: i64 = conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap_or(0);
        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap_or(0);

        (page_count * page_size) as usize
    }

    fn flush(&self) -> Result<()> {
        // SQLite auto-commits, but we can checkpoint WAL
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", [])
            .map_err(|e| Error::Storage(format!("sqlite checkpoint error: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeId, Predicate, Value};

    #[test]
    fn test_sqlite_backend() {
        let backend = SqliteBackend::memory().unwrap();

        let triple = Triple::new(
            NodeId::named("sqlite:test"),
            Predicate::named("property"),
            Value::literal("value"),
        );
        let id = triple.id();

        // Insert
        backend.put(&id, &triple).unwrap();
        assert_eq!(backend.count(), 1);

        // Get
        let retrieved = backend.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.subject, triple.subject);

        // Delete
        backend.delete(&id).unwrap();
        assert_eq!(backend.count(), 0);
    }

    #[test]
    fn test_sqlite_iter_all() {
        let backend = SqliteBackend::memory().unwrap();

        for i in 0..10 {
            let triple = Triple::new(
                NodeId::named(format!("node:{}", i)),
                Predicate::named("index"),
                Value::integer(i),
            );
            backend.put(&triple.id(), &triple).unwrap();
        }

        let all = backend.iter_all().unwrap();
        assert_eq!(all.len(), 10);
    }
}
