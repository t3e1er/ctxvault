//! Call sites persistence methods for Tier 2 progressive disclosure.

use rusqlite::params;
use std::collections::HashSet;

use groundcontrol_common::types::CallSiteRecord;
use groundcontrol_common::{Error, Result};

use super::Store;

impl Store {
    /// Batch insert call-site records within a transaction.
    pub fn insert_call_sites(&self, call_sites: &[CallSiteRecord]) -> Result<()> {
        if call_sites.is_empty() {
            return Ok(());
        }
        let conn = self.conn();
        if conn.is_autocommit() {
            let tx = conn.unchecked_transaction().map_err(|e| Error::Database(e.to_string()))?;
            {
                let mut stmt = tx
                    .prepare_cached(
                        "INSERT INTO call_sites (caller_scope, file_path, line, call_snippet, callee_name)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                    )
                    .map_err(|e| Error::Database(e.to_string()))?;

                for cs in call_sites {
                    stmt.execute(params![
                        cs.caller_scope,
                        cs.file_path,
                        cs.line as i64,
                        cs.call_snippet,
                        cs.callee_name,
                    ])
                    .map_err(|e| Error::Database(e.to_string()))?;
                }
            }
            tx.commit().map_err(|e| Error::Database(e.to_string()))?;
        } else {
            let mut stmt = conn
                .prepare_cached(
                    "INSERT INTO call_sites (caller_scope, file_path, line, call_snippet, callee_name)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| Error::Database(e.to_string()))?;

            for cs in call_sites {
                stmt.execute(params![
                    cs.caller_scope,
                    cs.file_path,
                    cs.line as i64,
                    cs.call_snippet,
                    cs.callee_name,
                ])
                .map_err(|e| Error::Database(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Delete all call-site records for a given file.
    pub fn delete_call_sites_for_file(&self, file_path: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM call_sites WHERE file_path = ?1", params![file_path])
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// Retrieve bounded call-site preambles for a given target callee symbol or scope path.
    ///
    /// Matches either the exact `callee_scope_path` or its normalized leaf symbol name.
    /// Deduplicates by caller scope and file path to maximize caller diversity.
    pub fn get_call_sites_for_symbol(
        &self,
        callee_scope_path: &str,
        max_callers: usize,
    ) -> Result<Vec<CallSiteRecord>> {
        if max_callers == 0 {
            return Ok(Vec::new());
        }

        let leaf = callee_scope_path
            .rsplit("::")
            .next()
            .unwrap_or(callee_scope_path)
            .rsplit(" > ")
            .next()
            .unwrap_or(callee_scope_path)
            .rsplit('.')
            .next()
            .unwrap_or(callee_scope_path)
            .trim();

        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT caller_scope, file_path, line, call_snippet, callee_name
                 FROM call_sites
                 WHERE callee_name = ?1
                    OR callee_name = ?2
                    OR callee_name LIKE ('% > ' || ?2)
                    OR callee_name LIKE ('%::' || ?2)
                    OR callee_name LIKE ('%.' || ?2)
                 ORDER BY id ASC",
            )
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![callee_scope_path, leaf])
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut results = Vec::new();
        let mut seen_callers = HashSet::new();

        while let Some(row) = rows.next().map_err(|e| Error::Database(e.to_string()))? {
            let caller_scope: String = row.get(0).map_err(|e| Error::Database(e.to_string()))?;
            let file_path: String = row.get(1).map_err(|e| Error::Database(e.to_string()))?;
            let line_i64: i64 = row.get(2).map_err(|e| Error::Database(e.to_string()))?;
            let call_snippet: String = row.get(3).map_err(|e| Error::Database(e.to_string()))?;
            let callee_name: String = row.get(4).map_err(|e| Error::Database(e.to_string()))?;

            if seen_callers.insert((caller_scope.clone(), file_path.clone())) {
                results.push(CallSiteRecord {
                    caller_scope,
                    file_path,
                    line: line_i64 as usize,
                    call_snippet,
                    callee_name,
                });
                if results.len() >= max_callers {
                    break;
                }
            }
        }

        Ok(results)
    }
}
