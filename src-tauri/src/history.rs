use std::path::Path;

use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    pub id: i64,
    pub ts: i64,
    pub kind: String,
    pub app_id: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub meta: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStats {
    pub total: i64,
    pub layout: i64,
    pub typo: i64,
    pub dictation: i64,
    pub llm: i64,
    pub replace: i64,
    pub case: i64,
    pub saved_keystrokes: i64,
    pub dictation_seconds: i64,
    pub top_apps: Vec<(String, i64)>,
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY,
            ts INTEGER NOT NULL,
            kind TEXT NOT NULL,
            app_id TEXT,
            before TEXT,
            after TEXT,
            meta TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts);
        CREATE INDEX IF NOT EXISTS idx_events_kind ON events(kind);",
    )?;
    Ok(connection)
}

pub fn record(
    connection: &Connection,
    kind: &str,
    app_id: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
    meta: Option<&str>,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO events (ts, kind, app_id, before, after, meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            now_millis(),
            kind,
            app_id,
            before,
            after,
            meta
        ],
    )?;
    Ok(())
}

pub fn list(connection: &Connection, limit: usize, kind: Option<&str>) -> rusqlite::Result<Vec<HistoryEvent>> {
    let limit = limit.clamp(1, 500) as i64;
    let mut events = Vec::new();

    if let Some(kind) = kind {
        let mut statement = connection.prepare(
            "SELECT id, ts, kind, app_id, before, after, meta FROM events WHERE kind = ?1 ORDER BY ts DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![kind, limit], map_event)?;
        for row in rows {
            events.push(row?);
        }
    } else {
        let mut statement = connection.prepare(
            "SELECT id, ts, kind, app_id, before, after, meta FROM events ORDER BY ts DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], map_event)?;
        for row in rows {
            events.push(row?);
        }
    }

    Ok(events)
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEvent> {
    Ok(HistoryEvent {
        id: row.get(0)?,
        ts: row.get(1)?,
        kind: row.get(2)?,
        app_id: row.get(3)?,
        before: row.get(4)?,
        after: row.get(5)?,
        meta: row.get(6)?,
    })
}

pub fn stats(connection: &Connection) -> rusqlite::Result<HistoryStats> {
    let mut stats = HistoryStats::default();

    stats.total = connection.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
    for (kind, field) in [
        ("layout", &mut stats.layout),
        ("typo", &mut stats.typo),
        ("dictation", &mut stats.dictation),
        ("llm", &mut stats.llm),
        ("replace", &mut stats.replace),
        ("case", &mut stats.case),
    ] {
        *field = connection.query_row(
            "SELECT COUNT(*) FROM events WHERE kind = ?1",
            params![kind],
            |row| row.get(0),
        )?;
    }

    let mut statement = connection.prepare(
        "SELECT app_id, COUNT(*) FROM events WHERE app_id IS NOT NULL GROUP BY app_id ORDER BY COUNT(*) DESC LIMIT 5",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        stats.top_apps.push(row?);
    }

    stats.saved_keystrokes = stats.layout * 6 + stats.typo * 3 + stats.replace * 2;
    stats.dictation_seconds = connection
        .query_row(
            "SELECT COALESCE(SUM(json_extract(meta, '$.seconds')), 0) FROM events WHERE kind = 'dictation'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);

    Ok(stats)
}

pub fn clear(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute("DELETE FROM events", [])?;
    Ok(())
}

pub fn export_csv(connection: &Connection) -> rusqlite::Result<String> {
    let mut output = String::from("ts,kind,app_id,before,after\n");
    let mut statement = connection.prepare(
        "SELECT ts, kind, app_id, before, after FROM events ORDER BY ts ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    for row in rows {
        let (ts, kind, app_id, before, after) = row?;
        output.push_str(&format!(
            "{},{},{},{},{}\n",
            ts,
            csv_escape(&kind),
            csv_escape(app_id.as_deref().unwrap_or("")),
            csv_escape(before.as_deref().unwrap_or("")),
            csv_escape(after.as_deref().unwrap_or(""))
        ));
    }

    Ok(output)
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
