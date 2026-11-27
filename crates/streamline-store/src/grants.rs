use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use streamline_core::PrincipalRole;

use crate::error::StoreError;
use crate::models::{AccessGrant, CreateGrantRequest, Principal};
use crate::StreamlineStore;

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub(crate) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn record_audit_tx(
    conn: &Connection,
    principal_id: &str,
    action: &str,
    target_id: &str,
    outcome: &str,
    detail: serde_json::Value,
) -> Result<(), StoreError> {
    let event_id = format!("evt-{}", uuid::Uuid::new_v4());
    let created_at = now_rfc3339();
    let detail_json = serde_json::to_string(&detail)?;
    let event_bytes =
        principal_id.len() + action.len() + target_id.len() + outcome.len() + detail_json.len();
    if event_bytes > crate::limits::MAX_AUDIT_EVENT_BYTES {
        return Err(StoreError::InvalidInput("audit event exceeds 2 MiB".into()));
    }
    conn.execute(
        "INSERT INTO audit_events (
            event_id, principal_id, action, target_id, outcome, detail_json, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            event_id,
            principal_id,
            action,
            target_id,
            outcome,
            detail_json,
            created_at
        ],
    )?;
    Ok(())
}

fn is_expired(expires_at: Option<&str>) -> bool {
    let Some(raw) = expires_at else {
        return false;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    match DateTime::parse_from_rfc3339(trimmed) {
        Ok(parsed) => parsed.with_timezone(&Utc) <= Utc::now(),
        Err(_) => true,
    }
}

fn normalize_optional_family(raw: Option<String>) -> Option<String> {
    raw.map(|val| val.trim().to_string())
        .filter(|val| !val.is_empty())
}

fn map_grant_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AccessGrant> {
    let granted_tags_json: String = row.get(4)?;
    let role_str: String = row.get(3)?;
    let can_write_int: i64 = row.get(5)?;
    let can_dispatch_int: i64 = row.get(6)?;
    let granted_tags: Vec<String> = serde_json::from_str(&granted_tags_json).unwrap_or_default();
    let principal_role = role_str
        .parse::<PrincipalRole>()
        .unwrap_or(PrincipalRole::Guest);
    let model_family = normalize_optional_family(row.get(10)?);

    Ok(AccessGrant {
        grant_id: row.get(0)?,
        principal_id: row.get(1)?,
        principal_handle: row.get(2)?,
        principal_role,
        granted_tags,
        can_write: can_write_int != 0,
        can_dispatch: can_dispatch_int != 0,
        acked_rules_sha256: row.get(7)?,
        expires_at: row.get(8)?,
        revoked_at: row.get(9)?,
        model_family,
    })
}

pub(crate) fn load_active_grant_tx(
    conn: &Connection,
    grant_id: &str,
) -> Result<AccessGrant, StoreError> {
    let grant = conn
        .query_row(
            "SELECT g.grant_id, g.principal_id, p.handle, p.role, g.granted_tags_json,
                    g.can_write, g.can_dispatch, g.acked_rules_sha256, g.expires_at,
                    g.revoked_at, g.model_family
             FROM access_grants g
             JOIN principals p ON p.id = g.principal_id
             WHERE g.grant_id = ?1",
            params![grant_id],
            map_grant_row,
        )
        .optional()?
        .ok_or(StoreError::Unauthorized)?;

    if grant.revoked_at.is_some() {
        return Err(StoreError::GrantRevoked);
    }
    if is_expired(grant.expires_at.as_deref()) {
        return Err(StoreError::GrantExpired);
    }
    Ok(grant)
}

pub(crate) fn current_rules_sha256_tx(conn: &Connection) -> Result<String, StoreError> {
    conn.query_row(
        "SELECT current_sha256 FROM resources
         WHERE shelf_slug = 'standards' AND slug = 'rules' AND deleted_at IS NULL",
        [],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| StoreError::NotFound("standards/rules resource not found".to_string()))
}

pub(crate) fn require_rules_acked_tx(
    conn: &Connection,
    grant_id: &str,
) -> Result<AccessGrant, StoreError> {
    let grant = load_active_grant_tx(conn, grant_id)?;
    let current_sha = current_rules_sha256_tx(conn)?;
    if grant.acked_rules_sha256.as_deref() != Some(current_sha.as_str()) {
        return Err(StoreError::RulesAckRequired {
            current_rules_sha256: current_sha,
        });
    }
    Ok(grant)
}

pub(crate) fn grant_has_tag_access(
    grant: &AccessGrant,
    shelf_slug: &str,
    required_tag: &str,
    project_id: Option<&str>,
) -> bool {
    if grant
        .granted_tags
        .iter()
        .any(|tag_item| tag_item == "*" || tag_item == required_tag)
    {
        return true;
    }
    if shelf_slug == "projects" {
        if let Some(pid) = project_id {
            let scoped = format!("project:{pid}");
            if grant
                .granted_tags
                .iter()
                .any(|tag_item| tag_item == &scoped)
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn missing_tag_for_scope(
    shelf_slug: &str,
    required_tag: &str,
    project_id: Option<&str>,
) -> String {
    if shelf_slug == "projects" {
        if let Some(pid) = project_id {
            return format!("project:{pid}");
        }
    }
    required_tag.to_string()
}

pub(crate) fn check_shelf_access_tx(
    conn: &Connection,
    grant: &AccessGrant,
    shelf_slug: &str,
    project_id: Option<&str>,
) -> Result<(), StoreError> {
    let required_tag: String = conn
        .query_row(
            "SELECT required_tag FROM shelves WHERE slug = ?1",
            params![shelf_slug],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("shelf '{shelf_slug}' not found")))?;

    if grant_has_tag_access(grant, shelf_slug, &required_tag, project_id) {
        return Ok(());
    }

    let missing_tag = missing_tag_for_scope(shelf_slug, &required_tag, project_id);
    Err(StoreError::ForbiddenTag {
        required_tag: missing_tag,
    })
}

fn read_principal_tx(conn: &Connection, id: &str) -> Result<Principal, StoreError> {
    conn.query_row(
        "SELECT id, handle, role, created_at FROM principals WHERE id = ?1",
        params![id],
        |row| {
            let role_raw: String = row.get(2)?;
            let role = role_raw
                .parse::<PrincipalRole>()
                .unwrap_or(PrincipalRole::Guest);
            Ok(Principal {
                id: row.get(0)?,
                handle: row.get(1)?,
                role,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| StoreError::NotFound(format!("principal '{id}' not found")))
}

impl StreamlineStore {
    /// Creates a new principal record.
    pub fn create_principal(
        &self,
        id: &str,
        handle: &str,
        role: PrincipalRole,
    ) -> Result<Principal, StoreError> {
        let trimmed_id = id.trim();
        let trimmed_handle = handle.trim();
        if trimmed_id.is_empty() || trimmed_handle.is_empty() {
            return Err(StoreError::InvalidInput(
                "principal id and handle must be non-empty".to_string(),
            ));
        }
        let now = now_rfc3339();
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO principals (id, handle, role, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![trimmed_id, trimmed_handle, role.as_str(), now],
        )
        .map_err(|err| match err {
            rusqlite::Error::SqliteFailure(sqlite_err, _)
                if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                StoreError::Conflict(format!("principal '{trimmed_id}' already exists"))
            }
            other => StoreError::Sqlite(other),
        })?;

        Ok(Principal {
            id: trimmed_id.to_string(),
            handle: trimmed_handle.to_string(),
            role,
            created_at: now,
        })
    }

    /// Inserts or updates a principal's handle and role.
    pub fn upsert_principal(
        &self,
        id: &str,
        handle: &str,
        role: PrincipalRole,
    ) -> Result<Principal, StoreError> {
        let trimmed_id = id.trim();
        let trimmed_handle = handle.trim();
        if trimmed_id.is_empty() || trimmed_handle.is_empty() {
            return Err(StoreError::InvalidInput(
                "principal id and handle must be non-empty".to_string(),
            ));
        }
        let now = now_rfc3339();
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO principals (id, handle, role, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET handle = excluded.handle, role = excluded.role",
            params![trimmed_id, trimmed_handle, role.as_str(), now],
        )?;
        read_principal_tx(&conn, trimmed_id)
    }

    /// Fetches a principal by id.
    pub fn get_principal(&self, id: &str) -> Result<Principal, StoreError> {
        let conn = self.lock_conn()?;
        read_principal_tx(&conn, id)
    }

    /// Issues a new tag-scoped access grant after hashing the raw credential with SHA-256.
    pub fn create_grant(&self, req: CreateGrantRequest) -> Result<AccessGrant, StoreError> {
        if req.raw_credential.trim().is_empty() {
            return Err(StoreError::InvalidInput(
                "raw_credential must be non-empty".to_string(),
            ));
        }
        let grant_id = req
            .grant_id
            .filter(|val| !val.trim().is_empty())
            .unwrap_or_else(|| format!("grant-{}", uuid::Uuid::new_v4()));
        let credential_hash = sha256_hex(req.raw_credential.as_bytes());
        let tags_json = serde_json::to_string(&req.granted_tags)?;
        let model_family = normalize_optional_family(req.model_family);

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;

        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM principals WHERE id = ?1)",
            params![req.principal_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StoreError::NotFound(format!(
                "principal '{}' not found",
                req.principal_id
            )));
        }

        tx.execute(
            "INSERT INTO access_grants (
                grant_id, principal_id, credential_hash, granted_tags_json,
                can_write, can_dispatch, acked_rules_sha256, expires_at, revoked_at, model_family
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, ?8)",
            params![
                grant_id,
                req.principal_id,
                credential_hash,
                tags_json,
                if req.can_write { 1 } else { 0 },
                if req.can_dispatch { 1 } else { 0 },
                req.expires_at,
                model_family,
            ],
        )
        .map_err(|err| match err {
            rusqlite::Error::SqliteFailure(sqlite_err, _)
                if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                StoreError::Conflict("duplicate grant_id or credential".to_string())
            }
            other => StoreError::Sqlite(other),
        })?;

        record_audit_tx(
            &tx,
            &req.principal_id,
            "create_grant",
            &grant_id,
            "ok",
            serde_json::json!({
                "granted_tags": req.granted_tags,
                "can_write": req.can_write,
                "can_dispatch": req.can_dispatch,
                "model_family": model_family,
            }),
        )?;

        let grant = load_active_grant_tx(&tx, &grant_id)?;
        tx.commit()?;
        Ok(grant)
    }

    /// Authenticates a raw credential by SHA-256 digest lookup and validates expiry/revocation.
    pub fn authenticate_grant(&self, raw_credential: &str) -> Result<AccessGrant, StoreError> {
        if raw_credential.trim().is_empty() {
            return Err(StoreError::Unauthorized);
        }
        let digest = sha256_hex(raw_credential.as_bytes());
        let conn = self.lock_conn()?;
        let grant = conn
            .query_row(
                "SELECT g.grant_id, g.principal_id, p.handle, p.role, g.granted_tags_json,
                        g.can_write, g.can_dispatch, g.acked_rules_sha256, g.expires_at,
                        g.revoked_at, g.model_family
                 FROM access_grants g
                 JOIN principals p ON p.id = g.principal_id
                 WHERE g.credential_hash = ?1",
                params![digest],
                map_grant_row,
            )
            .optional()?
            .ok_or(StoreError::Unauthorized)?;

        if grant.revoked_at.is_some() {
            return Err(StoreError::GrantRevoked);
        }
        if is_expired(grant.expires_at.as_deref()) {
            return Err(StoreError::GrantExpired);
        }
        Ok(grant)
    }

    /// Fetches an active grant by its `grant_id`.
    pub fn get_grant(&self, grant_id: &str) -> Result<AccessGrant, StoreError> {
        let conn = self.lock_conn()?;
        load_active_grant_tx(&conn, grant_id)
    }

    /// Revokes an access grant immediately.
    pub fn revoke_grant(&self, grant_id: &str) -> Result<(), StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let now = now_rfc3339();
        let affected = tx.execute(
            "UPDATE access_grants SET revoked_at = ?1 WHERE grant_id = ?2 AND revoked_at IS NULL",
            params![now, grant_id],
        )?;
        if affected == 0 {
            return Err(StoreError::NotFound(format!(
                "active grant '{grant_id}' not found"
            )));
        }
        record_audit_tx(
            &tx,
            "system",
            "revoke_grant",
            grant_id,
            "ok",
            serde_json::json!({}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Returns the SHA-256 digest of the active `standards/rules` resource.
    pub fn current_rules_sha256(&self) -> Result<String, StoreError> {
        let conn = self.lock_conn()?;
        current_rules_sha256_tx(&conn)
    }

    /// Records acknowledgement of the active `standards/rules` SHA-256 digest for `grant_id`.
    pub fn ack_rules(&self, grant_id: &str, rules_sha256: &str) -> Result<AccessGrant, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = load_active_grant_tx(&tx, grant_id)?;
        let current_sha = current_rules_sha256_tx(&tx)?;
        if rules_sha256.trim() != current_sha {
            return Err(StoreError::RulesAckRequired {
                current_rules_sha256: current_sha,
            });
        }
        tx.execute(
            "UPDATE access_grants SET acked_rules_sha256 = ?1 WHERE grant_id = ?2",
            params![current_sha, grant.grant_id],
        )?;
        record_audit_tx(
            &tx,
            &grant.principal_id,
            "ack_rules",
            &grant.grant_id,
            "ok",
            serde_json::json!({ "rules_sha256": current_sha }),
        )?;
        let updated = load_active_grant_tx(&tx, grant_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Verifies that the grant has acknowledged the current `standards/rules` digest,
    /// returning `StoreError::RulesAckRequired` (`412` semantics) otherwise.
    pub fn require_rules_acked(&self, grant_id: &str) -> Result<AccessGrant, StoreError> {
        let conn = self.lock_conn()?;
        require_rules_acked_tx(&conn, grant_id)
    }
}
