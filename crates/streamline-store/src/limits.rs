//! Store-wide growth and materialization limits. Reject rather than truncate history or results.

use rusqlite::Connection;
use serde::Serialize;

use crate::StoreError;

pub(crate) const MAX_DATABASE_BYTES: i64 = 4 * 1024 * 1024 * 1024;
pub(crate) const MAX_AUDIT_EVENT_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_RESOURCE_REVISIONS: i64 = 4096;
pub(crate) const MAX_RESOURCE_BODY_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_RESOURCE_METADATA_BYTES: usize = 64 * 1024;
pub(crate) const MAX_RESULT_ROWS: usize = 20_000;
pub(crate) const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_VAULT_BYTES: usize = 256 * 1024 * 1024;

// SQLite rounds the limit to whole pages and will not shrink an existing database.
// Apply this on every connection, before migrations. WAL and backups need separate disk quotas.
pub(crate) fn limit_database(conn: &Connection, max_bytes: i64) -> Result<(), StoreError> {
    let page_size: i64 = conn.pragma_query_value(None, "page_size", |row| row.get(0))?;
    conn.pragma_update(None, "max_page_count", max_bytes / page_size)?;
    Ok(())
}

pub(crate) fn next_revision(current: i64) -> Result<i64, StoreError> {
    if !(1..MAX_RESOURCE_REVISIONS).contains(&current) {
        return Err(StoreError::InvalidInput(
            "resource revision limit reached or invalid; contact the operator".into(),
        ));
    }
    Ok(current + 1)
}

pub(crate) struct Budget {
    rows: usize,
    bytes: usize,
    max_bytes: usize,
    operation: &'static str,
}

impl Budget {
    pub(crate) fn new(operation: &'static str, max_bytes: usize) -> Self {
        Self {
            rows: 0,
            bytes: 0,
            max_bytes,
            operation,
        }
    }

    pub(crate) fn add(&mut self, bytes: usize) -> Result<(), StoreError> {
        if self.rows >= MAX_RESULT_ROWS || bytes > self.max_bytes.saturating_sub(self.bytes) {
            return Err(StoreError::InvalidInput(format!(
                "{} exceeds store collection limit; narrow the scope or contact the operator",
                self.operation
            )));
        }
        self.rows += 1;
        self.bytes += bytes;
        Ok(())
    }

    pub(crate) fn push<T: Serialize>(
        &mut self,
        out: &mut Vec<T>,
        item: T,
    ) -> Result<(), StoreError> {
        self.add(serialized_len(&item)?.saturating_add(1))?;
        out.push(item);
        Ok(())
    }
}

/// Measure serialized size without allocating a second copy of the collection item.
pub(crate) fn serialized_len<T: Serialize>(item: &T) -> Result<usize, StoreError> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, item)?;
    Ok(count.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_accepts_exact_bound_and_rejects_excess() {
        let mut budget = Budget::new("test", 4);
        let mut values = Vec::new();
        budget.push(&mut values, "a").unwrap();
        assert!(budget.push(&mut values, "b").is_err());
        assert_eq!(values, vec!["a"]);
        let mut rows = Budget::new("test", usize::MAX);
        for _ in 0..MAX_RESULT_ROWS {
            rows.add(0).unwrap();
        }
        assert!(rows.add(0).is_err());
    }

    #[test]
    fn revision_boundary_preserves_last_allowed_revision() {
        assert_eq!(
            next_revision(MAX_RESOURCE_REVISIONS - 1).unwrap(),
            MAX_RESOURCE_REVISIONS
        );
        assert!(next_revision(MAX_RESOURCE_REVISIONS).is_err());
        assert!(next_revision(i64::MAX).is_err());
        assert!(next_revision(0).is_err());
    }

    #[test]
    fn database_limit_is_enforced_in_pages() {
        let conn = Connection::open_in_memory().unwrap();
        let page_size: i64 = conn
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .unwrap();
        limit_database(&conn, 16 * page_size).unwrap();
        let actual: i64 = conn
            .pragma_query_value(None, "max_page_count", |row| row.get(0))
            .unwrap();
        assert_eq!(actual, 16);
        conn.execute("CREATE TABLE sample (value BLOB)", [])
            .unwrap();
        conn.execute("INSERT INTO sample VALUES (zeroblob(128))", [])
            .unwrap();
        assert!(conn
            .execute("INSERT INTO sample VALUES (zeroblob(?1))", [16 * page_size])
            .is_err());
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM sample", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }
}
