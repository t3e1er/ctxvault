---
title: "INC-001: SQLite Index Lock Timeout During Concurrent CLI Client Scans"
template: incident_report
severity: sev3
date: 2026-08-29
resolved_by: "Trent"
tags:
  - incident
  - sqlite
  - concurrency
---

# INC-001: SQLite Index Lock Timeout During Concurrent CLI Client Scans

## Summary
During parallel client queries in CLI mode, secondary processes failed to acquire SQLite read locks with `database is locked (5)`.

## Timeline
- **14:00**: Automated benchmark initiated 8 parallel reader processes.
- **14:02**: 3 reader processes exited with `rusqlite::Error::SqliteFailure(5)`.
- **14:15**: PRAGMA settings diagnosed; default `DELETE` journal mode caused exclusive file locking.
- **14:30**: Enabled SQLite `WAL` mode and increased `busy_timeout` to 5000ms.

## Root Cause
Default rollback journal mode locks the entire database file during write transactions, preventing concurrent readers from inspecting index metadata.

Referenced by: [[decisions/adr-001-graph-engine.md]].

## Action Items
1. Enable `PRAGMA journal_mode = WAL;` on all database connections.
2. Configure `PRAGMA busy_timeout = 5000;`.
3. Add multi-process stress test to CI suite.
