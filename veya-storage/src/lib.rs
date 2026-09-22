//! Local-only SQLite persistence for ClipboardRecord / PasteTrigger / Application.
//!
//! Raw events are never merged. Aggregation is a read-model concern (veya-core).

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use veya_core::{ClipboardRecord, PasteMethod, PasteTriggerRecord, SourceConfidence};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS application (
                exe TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                path TEXT NOT NULL DEFAULT '',
                icon BLOB,
                excluded INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS clipboard_record (
                sequence INTEGER PRIMARY KEY,
                content_type TEXT NOT NULL,
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                source_app TEXT NOT NULL,
                source_pid INTEGER NOT NULL,
                source_window TEXT NOT NULL DEFAULT '',
                source_confidence TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS paste_trigger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clipboard_record_id INTEGER NOT NULL
                    REFERENCES clipboard_record(sequence) ON DELETE CASCADE,
                target_app TEXT NOT NULL,
                target_pid INTEGER NOT NULL,
                target_window TEXT NOT NULL DEFAULT '',
                method TEXT NOT NULL,
                triggered_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_paste_record ON paste_trigger(clipboard_record_id);
            CREATE INDEX IF NOT EXISTS idx_record_hash ON clipboard_record(content_hash);
            CREATE INDEX IF NOT EXISTS idx_record_created ON clipboard_record(created_at_ms);
            "#,
        )
    }

    pub fn insert_record(&mut self, rec: &ClipboardRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO clipboard_record (
                sequence, content_type, content, content_hash,
                source_app, source_pid, source_window, source_confidence, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(sequence) DO UPDATE SET
                content=excluded.content,
                content_hash=excluded.content_hash,
                source_app=excluded.source_app,
                source_pid=excluded.source_pid,
                source_window=excluded.source_window,
                source_confidence=excluded.source_confidence,
                created_at_ms=excluded.created_at_ms
            "#,
            params![
                rec.sequence as i64,
                rec.content_type,
                rec.content,
                rec.content_hash,
                rec.source_app,
                rec.source_pid as i64,
                rec.source_window,
                rec.source_confidence.as_str(),
                rec.created_at_ms,
            ],
        )?;
        // Replace triggers for this record (simple and correct for v0.1 writes).
        self.conn.execute(
            "DELETE FROM paste_trigger WHERE clipboard_record_id = ?1",
            params![rec.sequence as i64],
        )?;
        for p in &rec.pastes {
            self.insert_paste(rec.sequence, p)?;
        }
        Ok(())
    }

    pub fn insert_paste(
        &mut self,
        record_sequence: u32,
        paste: &PasteTriggerRecord,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO paste_trigger (
                clipboard_record_id, target_app, target_pid, target_window, method, triggered_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                record_sequence as i64,
                paste.target_app,
                paste.target_pid as i64,
                paste.target_window,
                paste.method.label(),
                paste.triggered_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn load_all(&self) -> rusqlite::Result<Vec<ClipboardRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT sequence, content_type, content, content_hash,
                   source_app, source_pid, source_window, source_confidence, created_at_ms
            FROM clipboard_record
            ORDER BY created_at_ms ASC, sequence ASC
            "#,
        )?;
        let mut records: Vec<ClipboardRecord> = stmt
            .query_map([], |row| {
                Ok(ClipboardRecord {
                    sequence: row.get::<_, i64>(0)? as u32,
                    content_type: row.get(1)?,
                    content: row.get(2)?,
                    content_hash: row.get(3)?,
                    source_app: row.get(4)?,
                    source_pid: row.get::<_, i64>(5)? as u32,
                    source_window: row.get(6)?,
                    source_confidence: parse_confidence(&row.get::<_, String>(7)?),
                    created_at_ms: row.get(8)?,
                    pastes: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare(
            r#"
            SELECT clipboard_record_id, target_app, target_pid, target_window, method, triggered_at_ms
            FROM paste_trigger
            ORDER BY triggered_at_ms ASC, id ASC
            "#,
        )?;
        let pastes = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)? as u32,
                PasteTriggerRecord {
                    target_app: row.get(1)?,
                    target_pid: row.get::<_, i64>(2)? as u32,
                    target_window: row.get(3)?,
                    method: parse_method(&row.get::<_, String>(4)?),
                    triggered_at_ms: row.get(5)?,
                },
            ))
        })?;
        for item in pastes {
            let (seq, paste) = item?;
            if let Some(rec) = records.iter_mut().find(|r| r.sequence == seq) {
                rec.pastes.push(paste);
            }
        }
        Ok(records)
    }

    pub fn delete_record(&mut self, sequence: u32) -> rusqlite::Result<usize> {
        self.conn.execute(
            "DELETE FROM clipboard_record WHERE sequence = ?1",
            params![sequence as i64],
        )
    }

    pub fn clear(&mut self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "DELETE FROM paste_trigger; DELETE FROM clipboard_record;",
        )
    }

    /// Delete records older than `cutoff_ms`. Returns rows removed.
    pub fn purge_older_than(&mut self, cutoff_ms: i64) -> rusqlite::Result<usize> {
        self.conn.execute(
            "DELETE FROM clipboard_record WHERE created_at_ms < ?1",
            params![cutoff_ms],
        )
    }

    pub fn upsert_application(
        &mut self,
        exe: &str,
        display_name: &str,
        path: &str,
        excluded: bool,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO application (exe, display_name, path, excluded)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(exe) DO UPDATE SET
                display_name=excluded.display_name,
                path=excluded.path,
                excluded=excluded.excluded
            "#,
            params![exe, display_name, path, excluded as i64],
        )?;
        Ok(())
    }

    pub fn is_excluded(&self, exe: &str) -> rusqlite::Result<bool> {
        let row: Option<i64> = self
            .conn
            .query_row(
                "SELECT excluded FROM application WHERE exe = ?1",
                params![exe],
                |r| r.get(0),
            )
            .optional()?;
        Ok(matches!(row, Some(1)))
    }
}

fn parse_confidence(s: &str) -> SourceConfidence {
    match s {
        "exact" => SourceConfidence::Exact,
        "likely" => SourceConfidence::Likely,
        _ => SourceConfidence::Unknown,
    }
}

fn parse_method(s: &str) -> PasteMethod {
    match s {
        "Shift+Insert" => PasteMethod::ShiftInsert,
        _ => PasteMethod::CtrlV,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veya_core::{PasteMethod, SourceConfidence};

    fn rec(seq: u32) -> ClipboardRecord {
        ClipboardRecord {
            sequence: seq,
            content_type: "text".into(),
            content: format!("text-{seq}"),
            content_hash: format!("h{seq}"),
            source_app: "chrome.exe".into(),
            source_pid: 1,
            source_window: "Chrome".into(),
            source_confidence: SourceConfidence::Exact,
            created_at_ms: 1_000 + i64::from(seq),
            pastes: vec![PasteTriggerRecord {
                target_app: "notepad.exe".into(),
                target_pid: 2,
                target_window: String::new(),
                method: PasteMethod::CtrlV,
                triggered_at_ms: 2_000 + i64::from(seq),
            }],
        }
    }

    #[test]
    fn record_round_trips_with_paste_triggers() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_record(&rec(1)).unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "text-1");
        assert_eq!(loaded[0].pastes.len(), 1);
        assert_eq!(loaded[0].pastes[0].target_app, "notepad.exe");
        assert_eq!(loaded[0].source_confidence, SourceConfidence::Exact);
        assert_eq!(loaded[0].pastes[0].method, PasteMethod::CtrlV);
    }

    #[test]
    fn delete_record_cascades_paste_triggers() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_record(&rec(1)).unwrap();
        store.insert_record(&rec(2)).unwrap();
        store.delete_record(1).unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sequence, 2);
        assert_eq!(loaded[0].pastes.len(), 1);
    }

    #[test]
    fn raw_events_stay_separate_and_purge_by_age() {
        let mut store = Store::open_in_memory().unwrap();
        let mut a = rec(1);
        a.content_hash = "same".into();
        a.created_at_ms = 1_000;
        let mut b = rec(2);
        b.content_hash = "same".into();
        b.created_at_ms = 2_000;
        store.insert_record(&a).unwrap();
        store.insert_record(&b).unwrap();
        assert_eq!(store.load_all().unwrap().len(), 2);
        store.purge_older_than(1_500).unwrap();
        let left = store.load_all().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].sequence, 2);
    }
}
