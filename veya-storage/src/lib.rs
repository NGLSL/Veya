//! Local-only SQLite persistence for ClipboardRecord / PasteTrigger / Application.
//!
//! Raw events are never merged. Aggregation is a read-model concern (veya-core).

use std::{collections::HashMap, fmt::Write as _, path::Path};

use rusqlite::{params, types::Type, Connection, OptionalExtension};
use veya_core::{
    ClipboardPayload, ClipboardRecord, PasteMethod, PasteTriggerRecord, SourceConfidence,
};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> rusqlite::Result<()> {
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            r#"
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
                created_at_ms INTEGER NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 0,
                payload BLOB,
                image_width INTEGER,
                image_height INTEGER
            );
            CREATE TABLE IF NOT EXISTS paste_trigger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clipboard_record_id INTEGER NOT NULL
                    REFERENCES clipboard_record(sequence) ON DELETE CASCADE,
                target_app TEXT NOT NULL,
                target_pid INTEGER NOT NULL,
                target_window TEXT NOT NULL DEFAULT '',
                method TEXT NOT NULL,
                confidence TEXT NOT NULL DEFAULT 'hotkey-observed',
                triggered_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_paste_record ON paste_trigger(clipboard_record_id);
            CREATE INDEX IF NOT EXISTS idx_record_hash ON clipboard_record(content_hash);
            CREATE INDEX IF NOT EXISTS idx_record_created ON clipboard_record(created_at_ms);
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        if !has_column(&tx, "paste_trigger", "confidence")? {
            tx.execute_batch(
                "ALTER TABLE paste_trigger ADD COLUMN confidence TEXT NOT NULL DEFAULT 'hotkey-observed'",
            )?;
        }
        if !has_column(&tx, "clipboard_record", "pinned")? {
            tx.execute_batch(
                "ALTER TABLE clipboard_record ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        for (name, declaration) in [
            ("payload", "BLOB"),
            ("image_width", "INTEGER"),
            ("image_height", "INTEGER"),
        ] {
            if !has_column(&tx, "clipboard_record", name)? {
                tx.execute_batch(&format!(
                    "ALTER TABLE clipboard_record ADD COLUMN {name} {declaration}"
                ))?;
            }
        }
        tx.commit()
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO app_settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![key, value],
        )?;
        Ok(())
    }

    pub fn insert_record(&mut self, rec: &ClipboardRecord) -> rusqlite::Result<()> {
        let (blob, width, height) = encode_payload(&rec.payload)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            r#"
            INSERT INTO clipboard_record (
                sequence, content_type, content, content_hash,
                source_app, source_pid, source_window, source_confidence, created_at_ms, pinned,
                payload, image_width, image_height
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(sequence) DO UPDATE SET
                content_type=excluded.content_type,
                content=excluded.content,
                content_hash=excluded.content_hash,
                source_app=excluded.source_app,
                source_pid=excluded.source_pid,
                source_window=excluded.source_window,
                source_confidence=excluded.source_confidence,
                created_at_ms=excluded.created_at_ms,
                pinned=excluded.pinned,
                payload=excluded.payload,
                image_width=excluded.image_width,
                image_height=excluded.image_height
            "#,
            params![
                rec.sequence as i64,
                rec.payload.kind(),
                rec.payload.display_text(),
                rec.content_hash,
                rec.source_app,
                rec.source_pid as i64,
                rec.source_window,
                rec.source_confidence.as_str(),
                rec.created_at_ms,
                rec.pinned as i64,
                blob,
                width,
                height,
            ],
        )?;
        // Keep payload and paste associations atomic across updates.
        tx.execute(
            "DELETE FROM paste_trigger WHERE clipboard_record_id = ?1",
            params![rec.sequence as i64],
        )?;
        for p in &rec.pastes {
            tx.execute(
                r#"
                INSERT INTO paste_trigger (
                    clipboard_record_id, target_app, target_pid, target_window, method, confidence, triggered_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    rec.sequence as i64,
                    p.target_app,
                    p.target_pid as i64,
                    p.target_window,
                    p.method.label(),
                    p.confidence.as_str(),
                    p.triggered_at_ms,
                ],
            )?;
        }
        tx.commit()
    }

    /// Return the number of raw clipboard records without loading their payloads.
    pub fn count_records(&self) -> rusqlite::Result<usize> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM clipboard_record", [], |row| {
                    row.get(0)
                })?;
        usize::try_from(count).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, Type::Integer, Box::new(error))
        })
    }

    /// Load one keyset page of raw records.
    ///
    /// `cursor` is the `(created_at_ms, sequence)` key of the last record from
    /// the preceding page. The next page advances in the direction selected by
    /// `newest_first`, so equal timestamps remain deterministic by sequence.
    /// Only paste associations belonging to records in this page are loaded.
    pub fn load_records_page(
        &self,
        cursor: Option<(i64, u32)>,
        limit: usize,
        newest_first: bool,
    ) -> rusqlite::Result<Vec<ClipboardRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let direction = if newest_first { "DESC" } else { "ASC" };
        let comparison = if newest_first { "<" } else { ">" };
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let sql = format!(
            r#"
            SELECT sequence, content_type, content, content_hash,
                   source_app, source_pid, source_window, source_confidence, created_at_ms, pinned,
                   payload, image_width, image_height
            FROM clipboard_record
            WHERE (?1 IS NULL
                   OR created_at_ms {comparison} ?1
                   OR (created_at_ms = ?1 AND sequence {comparison} ?2))
            ORDER BY created_at_ms {direction}, sequence {direction}
            LIMIT ?3
            "#,
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut records = match cursor {
            Some((created_at_ms, sequence)) => stmt
                .query_map(
                    params![created_at_ms, i64::from(sequence), limit],
                    decode_record,
                )?
                .collect::<Result<Vec<_>, _>>()?,
            None => stmt
                .query_map(
                    params![Option::<i64>::None, Option::<i64>::None, limit],
                    decode_record,
                )?
                .collect::<Result<Vec<_>, _>>()?,
        };
        drop(stmt);

        self.load_pastes_for_records(&mut records)?;
        Ok(records)
    }

    /// Load one raw clipboard record and its paste associations by sequence.
    pub fn load_record(&self, sequence: u32) -> rusqlite::Result<Option<ClipboardRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT sequence, content_type, content, content_hash,
                   source_app, source_pid, source_window, source_confidence, created_at_ms, pinned,
                   payload, image_width, image_height
            FROM clipboard_record
            WHERE sequence = ?1
            "#,
        )?;
        let mut record = stmt
            .query_row(params![i64::from(sequence)], decode_record)
            .optional()?;
        drop(stmt);

        if let Some(record) = record.as_mut() {
            self.load_pastes_for_records(std::slice::from_mut(record))?;
        }
        Ok(record)
    }

    pub fn load_all(&self) -> rusqlite::Result<Vec<ClipboardRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT sequence, content_type, content, content_hash,
                   source_app, source_pid, source_window, source_confidence, created_at_ms, pinned,
                   payload, image_width, image_height
            FROM clipboard_record
            ORDER BY created_at_ms ASC, sequence ASC
            "#,
        )?;
        let mut records: Vec<ClipboardRecord> = stmt
            .query_map([], |row| {
                let content_type: String = row.get(1)?;
                let content: String = row.get(2)?;
                let blob: Option<Vec<u8>> = row.get(10)?;
                let width: Option<i64> = row.get(11)?;
                let height: Option<i64> = row.get(12)?;
                let payload = decode_payload(&content_type, &content, blob, width, height)?;
                let content = payload.display_text();
                Ok(ClipboardRecord {
                    sequence: row.get::<_, i64>(0)? as u32,
                    content_type,
                    content,
                    payload,
                    content_hash: row.get(3)?,
                    source_app: row.get(4)?,
                    source_pid: row.get::<_, i64>(5)? as u32,
                    source_window: row.get(6)?,
                    source_confidence: parse_confidence(&row.get::<_, String>(7)?),
                    created_at_ms: row.get(8)?,
                    pinned: row.get::<_, i64>(9)? != 0,
                    pastes: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare(
            r#"
            SELECT clipboard_record_id, target_app, target_pid, target_window, method, confidence, triggered_at_ms
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
                    confidence: parse_paste_confidence(&row.get::<_, String>(5)?),
                    triggered_at_ms: row.get(6)?,
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

    fn load_pastes_for_records(&self, records: &mut [ClipboardRecord]) -> rusqlite::Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let positions: HashMap<u32, usize> = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.sequence, index))
            .collect();
        let sequences: Vec<i64> = records
            .iter()
            .map(|record| i64::from(record.sequence))
            .collect();

        // Keep the association query below SQLite's variable limit even if a
        // caller requests an unusually large page.
        for chunk in sequences.chunks(900) {
            let mut placeholders = String::new();
            for index in 0..chunk.len() {
                if index > 0 {
                    placeholders.push_str(", ");
                }
                write!(&mut placeholders, "?{}", index + 1).expect("writing SQL placeholder");
            }
            let sql = format!(
                r#"
                SELECT clipboard_record_id, target_app, target_pid, target_window,
                       method, confidence, triggered_at_ms
                FROM paste_trigger
                WHERE clipboard_record_id IN ({placeholders})
                ORDER BY clipboard_record_id ASC, triggered_at_ms ASC, id ASC
                "#,
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let pastes = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)? as u32,
                    PasteTriggerRecord {
                        target_app: row.get(1)?,
                        target_pid: row.get::<_, i64>(2)? as u32,
                        target_window: row.get(3)?,
                        method: parse_method(&row.get::<_, String>(4)?),
                        confidence: parse_paste_confidence(&row.get::<_, String>(5)?),
                        triggered_at_ms: row.get(6)?,
                    },
                ))
            })?;
            for item in pastes {
                let (sequence, paste) = item?;
                if let Some(&index) = positions.get(&sequence) {
                    records[index].pastes.push(paste);
                }
            }
        }
        Ok(())
    }

    pub fn delete_record(&mut self, sequence: u32) -> rusqlite::Result<usize> {
        self.conn.execute(
            "DELETE FROM clipboard_record WHERE sequence = ?1",
            params![sequence as i64],
        )
    }

    pub fn delete_records(&mut self, sequences: &[u32]) -> rusqlite::Result<usize> {
        let tx = self.conn.transaction()?;
        let mut deleted = 0;
        for sequence in sequences {
            deleted += tx.execute(
                "DELETE FROM clipboard_record WHERE sequence = ?1",
                params![*sequence as i64],
            )?;
        }
        tx.commit()?;
        Ok(deleted)
    }

    /// Apply a card-level pin action to its raw records atomically.
    pub fn set_pinned(&mut self, sequences: &[u32], pinned: bool) -> rusqlite::Result<usize> {
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for sequence in sequences {
            changed += tx.execute(
                "UPDATE clipboard_record SET pinned = ?1 WHERE sequence = ?2 AND pinned != ?1",
                params![pinned as i64, *sequence as i64],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn clear(&mut self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM clipboard_record", [])?;
        Ok(())
    }

    /// Delete records older than `cutoff_ms`. Returns rows removed.
    pub fn purge_older_than(&mut self, cutoff_ms: i64) -> rusqlite::Result<usize> {
        self.conn.execute(
            "DELETE FROM clipboard_record WHERE created_at_ms < ?1 AND pinned = 0",
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

    pub fn list_excluded(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT exe FROM application WHERE excluded = 1 ORDER BY exe")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    }
}

fn decode_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardRecord> {
    let content_type: String = row.get(1)?;
    let content: String = row.get(2)?;
    let blob: Option<Vec<u8>> = row.get(10)?;
    let width: Option<i64> = row.get(11)?;
    let height: Option<i64> = row.get(12)?;
    let payload = decode_payload(&content_type, &content, blob, width, height)?;
    let content = payload.display_text();
    Ok(ClipboardRecord {
        sequence: row.get::<_, i64>(0)? as u32,
        content_type,
        content,
        payload,
        content_hash: row.get(3)?,
        source_app: row.get(4)?,
        source_pid: row.get::<_, i64>(5)? as u32,
        source_window: row.get(6)?,
        source_confidence: parse_confidence(&row.get::<_, String>(7)?),
        created_at_ms: row.get(8)?,
        pinned: row.get::<_, i64>(9)? != 0,
        pastes: Vec::new(),
    })
}

fn has_column(conn: &Connection, table: &str, name: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == name {
            return Ok(true);
        }
    }
    Ok(false)
}

fn encode_payload(
    payload: &ClipboardPayload,
) -> rusqlite::Result<(Option<Vec<u8>>, Option<i64>, Option<i64>)> {
    match payload {
        ClipboardPayload::Text(_) => Ok((None, None, None)),
        ClipboardPayload::Files(paths) => serde_json::to_vec(paths)
            .map(|bytes| (Some(bytes), None, None))
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error))),
        ClipboardPayload::Image { png, width, height } => Ok((
            Some(png.clone()),
            Some(i64::from(*width)),
            Some(i64::from(*height)),
        )),
    }
}

fn invalid_payload(message: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        10,
        Type::Blob,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.to_string(),
        )),
    )
}

fn decode_payload(
    kind: &str,
    content: &str,
    blob: Option<Vec<u8>>,
    width: Option<i64>,
    height: Option<i64>,
) -> rusqlite::Result<ClipboardPayload> {
    match kind {
        "text" => Ok(ClipboardPayload::Text(content.to_string())),
        "files" => {
            let bytes = blob.ok_or_else(|| invalid_payload("missing file-list payload"))?;
            let paths: Vec<String> = serde_json::from_slice(&bytes).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(10, Type::Blob, Box::new(error))
            })?;
            if paths.is_empty() {
                return Err(invalid_payload("empty file-list payload"));
            }
            Ok(ClipboardPayload::Files(paths))
        }
        "image" => {
            let png = blob.ok_or_else(|| invalid_payload("missing image payload"))?;
            let width = width.and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0);
            let height = height
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0);
            match (width, height) {
                (Some(width), Some(height)) if !png.is_empty() => {
                    Ok(ClipboardPayload::Image { png, width, height })
                }
                _ => Err(invalid_payload("invalid image payload")),
            }
        }
        _ => Err(invalid_payload("unknown clipboard payload type")),
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

fn parse_paste_confidence(_s: &str) -> veya_core::PasteConfidence {
    // v0.1 has a single honest claim: hotkey observed, insertion unverified.
    veya_core::PasteConfidence::HotkeyObserved
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
            payload: ClipboardPayload::Text(format!("text-{seq}")),
            content_hash: format!("h{seq}"),
            source_app: "chrome.exe".into(),
            source_pid: 1,
            source_window: "Chrome".into(),
            source_confidence: SourceConfidence::Exact,
            created_at_ms: 1_000 + i64::from(seq),
            pinned: false,
            pastes: vec![PasteTriggerRecord {
                target_app: "notepad.exe".into(),
                target_pid: 2,
                target_window: String::new(),
                method: PasteMethod::CtrlV,
                confidence: veya_core::PasteConfidence::HotkeyObserved,
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

    #[test]
    fn pinning_multiple_raw_records_survives_reload_and_exempts_retention() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_record(&rec(1)).unwrap();
        store.insert_record(&rec(2)).unwrap();
        store.insert_record(&rec(3)).unwrap();
        assert_eq!(store.set_pinned(&[1, 2], true).unwrap(), 2);
        assert_eq!(store.purge_older_than(2_000).unwrap(), 1);
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().all(|record| record.pinned));

        assert_eq!(store.set_pinned(&[1, 2], false).unwrap(), 2);
        assert_eq!(store.purge_older_than(2_000).unwrap(), 2);
        assert!(store.load_all().unwrap().is_empty());
    }

    #[test]
    fn clear_also_removes_pinned_records() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_record(&rec(1)).unwrap();
        store.set_pinned(&[1], true).unwrap();
        store.clear().unwrap();
        assert!(store.load_all().unwrap().is_empty());
    }

    #[test]
    fn paged_history_uses_stable_keyset_cursor_in_both_directions() {
        let mut store = Store::open_in_memory().unwrap();
        for (sequence, created_at_ms) in [(1, 100), (2, 100), (3, 100), (4, 200), (5, 300)] {
            let mut record = rec(sequence);
            record.created_at_ms = created_at_ms;
            store.insert_record(&record).unwrap();
        }

        assert_eq!(store.count_records().unwrap(), 5);

        let oldest_first = store.load_records_page(None, 2, false).unwrap();
        assert_eq!(
            oldest_first
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        let oldest_second = store
            .load_records_page(
                Some((oldest_first[1].created_at_ms, oldest_first[1].sequence)),
                2,
                false,
            )
            .unwrap();
        assert_eq!(
            oldest_second
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            vec![3, 4]
        );
        let oldest_last = store
            .load_records_page(
                Some((oldest_second[1].created_at_ms, oldest_second[1].sequence)),
                2,
                false,
            )
            .unwrap();
        assert_eq!(
            oldest_last
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            vec![5]
        );
        assert!(store
            .load_records_page(
                Some((oldest_last[0].created_at_ms, oldest_last[0].sequence)),
                2,
                false,
            )
            .unwrap()
            .is_empty());

        let newest_first = store.load_records_page(None, 2, true).unwrap();
        assert_eq!(
            newest_first
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            vec![5, 4]
        );
        let newest_second = store
            .load_records_page(
                Some((newest_first[1].created_at_ms, newest_first[1].sequence)),
                2,
                true,
            )
            .unwrap();
        assert_eq!(
            newest_second
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            vec![3, 2]
        );
        let newest_last = store
            .load_records_page(
                Some((newest_second[1].created_at_ms, newest_second[1].sequence)),
                2,
                true,
            )
            .unwrap();
        assert_eq!(
            newest_last
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert!(store
            .load_records_page(
                Some((newest_last[0].created_at_ms, newest_last[0].sequence)),
                2,
                true,
            )
            .unwrap()
            .is_empty());
    }

    #[test]
    fn paged_and_single_record_reads_preserve_images_and_page_pastes() {
        let mut store = Store::open_in_memory().unwrap();
        let mut image = rec(1);
        image.created_at_ms = 100;
        image.payload = ClipboardPayload::Image {
            png: vec![137, 80, 78, 71, 1, 2, 3],
            width: 320,
            height: 240,
        };
        image.pastes[0].target_app = "image-target.exe".into();

        let mut later = rec(2);
        later.created_at_ms = 200;
        later.pastes[0].target_app = "later-target.exe".into();
        store.insert_record(&image).unwrap();
        store.insert_record(&later).unwrap();

        let page = store.load_records_page(None, 1, false).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].sequence, 1);
        assert_eq!(
            page[0].payload,
            ClipboardPayload::Image {
                png: vec![137, 80, 78, 71, 1, 2, 3],
                width: 320,
                height: 240,
            }
        );
        assert_eq!(page[0].pastes.len(), 1);
        assert_eq!(page[0].pastes[0].target_app, "image-target.exe");

        let loaded = store.load_record(1).unwrap().unwrap();
        assert_eq!(loaded, page[0]);
        assert!(store.load_record(999).unwrap().is_none());
    }

    #[test]
    fn migrates_legacy_text_records_with_unpinned_default() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE clipboard_record (
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
            CREATE TABLE paste_trigger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clipboard_record_id INTEGER NOT NULL,
                target_app TEXT NOT NULL,
                target_pid INTEGER NOT NULL,
                target_window TEXT NOT NULL DEFAULT '',
                method TEXT NOT NULL,
                triggered_at_ms INTEGER NOT NULL
            );
            INSERT INTO clipboard_record VALUES
                (42, 'text', 'legacy text', 'h42', 'legacy.exe', 7, '', 'exact', 1000);
            "#,
        )
        .unwrap();
        let mut store = Store { conn };
        store.migrate().unwrap();
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "legacy text");
        assert!(!loaded[0].pinned);
    }

    #[test]
    fn typed_payloads_round_trip_without_becoming_text() {
        let mut store = Store::open_in_memory().unwrap();
        let mut files = rec(1);
        files.payload =
            ClipboardPayload::Files(vec![r"C:\临时\a.txt".into(), r"C:\临时\folder".into()]);
        let mut image = rec(2);
        image.payload = ClipboardPayload::Image {
            png: vec![137, 80, 78, 71],
            width: 320,
            height: 240,
        };
        store.insert_record(&files).unwrap();
        store.insert_record(&image).unwrap();

        let loaded = store.load_all().unwrap();
        assert_eq!(loaded[0].payload, files.payload);
        assert_eq!(loaded[0].content_type, "files");
        assert_eq!(loaded[1].payload, image.payload);
        assert_eq!(loaded[1].content_type, "image");
    }

    #[test]
    fn failed_migration_keeps_existing_history_intact() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE clipboard_record (
                sequence INTEGER PRIMARY KEY,
                content_type TEXT NOT NULL,
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                source_app TEXT NOT NULL,
                source_pid INTEGER NOT NULL,
                source_window TEXT NOT NULL,
                source_confidence TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            INSERT INTO clipboard_record VALUES (1, 'text', 'keep me', 'h1', 'old.exe', 1, '', 'exact', 1);
            CREATE VIEW paste_trigger AS SELECT 1 AS clipboard_record_id;
            "#,
        )
        .unwrap();
        let mut store = Store { conn };
        assert!(store.migrate().is_err());
        let content: String = store
            .conn
            .query_row(
                "SELECT content FROM clipboard_record WHERE sequence = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(content, "keep me");
        assert!(!has_column(&store.conn, "clipboard_record", "pinned").unwrap());
    }
}
