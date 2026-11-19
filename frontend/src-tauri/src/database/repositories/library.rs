//! Local library reads. Canonical transcripts are #14's selected editable
//! projection, not archived candidates. No search result is an AI context input.
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::path::Path;

use super::person::{snippet_around, visible_summary_text};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMaterial {
    pub id: String,
    pub transcript_count: i64,
    pub has_notes: bool,
    pub has_summary: bool,
    pub summary_status: Option<String>,
    pub audio_available: bool,
}

pub fn audio_available(folder: Option<&str>) -> bool {
    let Some(folder) = folder else { return false };
    let dir = Path::new(folder);
    std::fs::read_dir(dir).ok().is_some_and(|entries| entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        path.is_file()
            && path.file_stem().and_then(|v| v.to_str()) == Some("audio")
            && path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
                ["mp4", "m4a", "mp3", "wav", "aac", "flac", "ogg", "webm"].contains(&v.to_ascii_lowercase().as_str())
            })
    }))
}

fn notes_text(markdown: Option<&str>, json: Option<&str>) -> String {
    use pulldown_cmark::{Event, Options, Parser, TagEnd};
    use serde_json::Value;

    fn inline(value: &Value) -> Option<String> {
        match value {
            Value::Null => Some(String::new()),
            Value::String(text) => Some(text.clone()),
            Value::Array(values) => values.iter().map(inline).collect(),
            Value::Object(object) => {
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    Some(text.into())
                } else {
                    object.get("content").and_then(inline)
                }
            }
            _ => None,
        }
    }
    fn blocks(value: &Value) -> Option<String> {
        let mut output = Vec::new();
        for block in value.as_array()? {
            // A syntactically valid cache/object is not a rich Notes document.
            block.get("type")?.as_str()?;
            let text = match block.get("content") {
                Some(content) if content.get("rows").is_some() => {
                    let mut rows = Vec::new();
                    for row in content.get("rows")?.as_array()? {
                        let cells = row.get("cells")?.as_array()?;
                        rows.push(cells.iter().map(inline).collect::<Option<Vec<_>>>()?.join("\t"));
                    }
                    rows.join("\n")
                }
                Some(content) => inline(content)?,
                None => String::new(),
            };
            output.push(text);
            if let Some(children) = block.get("children") {
                let children = blocks(children)?;
                if !children.is_empty() {
                    output.push(children);
                }
            }
        }
        Some(output.join("\n"))
    }
    if let Some(text) = json
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .as_ref()
        .filter(|value| value.as_array().is_some_and(|blocks| !blocks.is_empty()))
        .and_then(blocks)
    {
        // Adjacent styled/link runs are one visible inline string. Only actual
        // block/list/cell boundaries separate words; IDs/URLs/styles never enter
        // either matching or snippets. Valid empty rich content stays empty.
        return text.trim().into();
    }

    fn line_break(output: &mut String) {
        while output.ends_with('\t') {
            output.pop();
        }
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
    }
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut output = String::new();
    for event in Parser::new_ext(markdown.unwrap_or_default(), options) {
        match event {
            Event::Text(text) | Event::Code(text) => output.push_str(&text),
            Event::SoftBreak => output.push(' '),
            Event::HardBreak | Event::Rule => line_break(&mut output),
            Event::End(TagEnd::TableCell) => output.push('\t'),
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::BlockQuote(_)
                | TagEnd::CodeBlock | TagEnd::Item | TagEnd::TableHead
                | TagEnd::TableRow | TagEnd::Table,
            ) => line_break(&mut output),
            // Link/image destinations, titles, reference definitions, raw HTML
            // attributes and code language hints are metadata, not visible text.
            _ => {},
        }
    }
    output.trim().into()
}

pub async fn materials(pool: &SqlitePool) -> Result<Vec<LibraryMaterial>, sqlx::Error> {
    crate::post_call::store::projection::recover(pool,None).await.map_err(sqlx::Error::Protocol)?;
    let rows = sqlx::query(
        "SELECT m.id, m.folder_path, (SELECT COUNT(*) FROM transcripts t WHERE t.meeting_id=m.id) AS count, \
         n.notes_markdown, n.notes_json, s.result, s.status FROM meetings m \
         LEFT JOIN meeting_notes n ON n.meeting_id=m.id LEFT JOIN current_meeting_summaries s ON s.meeting_id=m.id",
    ).fetch_all(pool).await?;
    rows.into_iter().map(|row| {
        let raw: Option<String> = row.try_get("result")?;
        let notes: Option<String> = row.try_get("notes_markdown")?;
        let json: Option<String> = row.try_get("notes_json")?;
        let folder: Option<String> = row.try_get("folder_path")?;
        Ok(LibraryMaterial {
            id: row.try_get("id")?, transcript_count: row.try_get("count")?,
            has_notes: !notes_text(notes.as_deref(), json.as_deref()).trim().is_empty(),
            has_summary: raw.as_deref().and_then(visible_summary_text).is_some_and(|v| !v.trim().is_empty()),
            summary_status: row.try_get("status")?, audio_available: audio_available(folder.as_deref()),
        })
    }).collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySearchHit {
    pub kind: String,
    pub id: String,
    pub meeting_id: String,
    pub title: String,
    pub snippet: String,
    pub timestamp: String,
    pub transcript_id: Option<String>,
    pub speaker: Option<String>,
    pub audio_start_time: Option<f64>,
}

/// SQLite's built-in lower()/LIKE are ASCII-only. Scan bounded pages and match
/// visible text in Rust for RU/EN and literal %, _ and backslash. Keep only a
/// bounded result set, never materialize an entire transcript library or JSON.
pub async fn search(pool: &SqlitePool, query: &str, limit: usize, one_per_meeting: bool) -> Result<Vec<LibrarySearchHit>, sqlx::Error> {
    crate::post_call::store::projection::recover(pool,None).await.map_err(sqlx::Error::Protocol)?;
    let query = query.trim().to_lowercase();
    if query.is_empty() { return Ok(vec![]) }
    let limit = limit.clamp(1, 200);
    let mut tx = pool.begin().await?;
    let mut hits = Vec::new();
    for kind in ["meeting", "notes", "transcript", "summary"] {
        let sql = match kind {
            "meeting" => "SELECT m.id AS id,m.id AS meeting_id,m.title,m.title AS text,m.created_at AS timestamp,NULL AS speaker,NULL AS audio_start_time FROM meetings m ORDER BY m.created_at DESC,m.id LIMIT 256 OFFSET ?",
            "notes" => "SELECT m.id AS id,m.id AS meeting_id,m.title,COALESCE(n.notes_markdown,'') AS text,n.notes_json,m.created_at AS timestamp,NULL AS speaker,NULL AS audio_start_time FROM meeting_notes n JOIN meetings m ON m.id=n.meeting_id WHERE n.notes_markdown IS NOT NULL OR n.notes_json IS NOT NULL ORDER BY m.created_at DESC,m.id LIMIT 256 OFFSET ?",
            "transcript" => "SELECT t.id,m.id AS meeting_id,m.title,t.transcript AS text,m.created_at AS timestamp,t.speaker,t.audio_start_time FROM transcripts t JOIN meetings m ON m.id=t.meeting_id ORDER BY m.created_at DESC,m.id,COALESCE(t.audio_start_time,0),t.timestamp,t.id LIMIT 256 OFFSET ?",
            _ => "SELECT m.id AS id,m.id AS meeting_id,m.title,s.result AS text,m.created_at AS timestamp,NULL AS speaker,NULL AS audio_start_time FROM current_meeting_summaries s JOIN meetings m ON m.id=s.meeting_id WHERE s.result IS NOT NULL ORDER BY m.created_at DESC,m.id LIMIT 256 OFFSET ?",
        };
        let mut offset = 0_i64;
        let mut count = 0;
        loop {
            let rows = sqlx::query(sql).bind(offset).fetch_all(&mut *tx).await?;
            let length = rows.len();
            for row in rows {
                let raw: String = row.try_get("text")?;
                let text = if kind == "summary" { visible_summary_text(&raw).unwrap_or_default() }
                    else if kind == "notes" { let json: Option<String> = row.try_get("notes_json")?; notes_text(Some(&raw), json.as_deref()) }
                    else { raw };
                let speaker: Option<String> = row.try_get("speaker")?;
                if !text.to_lowercase().contains(&query) && !speaker.as_deref().unwrap_or("").to_lowercase().contains(&query) { continue }
                let meeting_id: String = row.try_get("meeting_id")?;
                if one_per_meeting && hits.iter().any(|h: &LibrarySearchHit| h.meeting_id == meeting_id) { continue }
                let id: String = row.try_get("id")?;
                hits.push(LibrarySearchHit {
                    kind: kind.into(), transcript_id: (kind == "transcript").then(|| id.clone()),
                    id: if kind == "transcript" { id } else { format!("{kind}-{id}") }, meeting_id,
                    title: row.try_get("title")?, snippet: snippet_around(&text, &query, 220),
                    timestamp: row.try_get("timestamp")?, speaker, audio_start_time: row.try_get("audio_start_time")?,
                });
                count += 1;
                if count >= limit || (one_per_meeting && hits.len() >= limit) { break }
            }
            if length < 256 || count >= limit || (one_per_meeting && hits.len() >= limit) { break }
            offset += length as i64;
        }
        if one_per_meeting && hits.len() >= limit { break }
    }
    tx.commit().await?;
    Ok(hits)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportNotes { pub source: &'static str, pub markdown: String, pub json: Option<serde_json::Value>, pub revision: i64 }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTranscript { pub id: String, pub text: String, pub speaker: Option<String>, pub audio_start_time: Option<f64> }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAction { pub text: String, pub done: bool, pub owner: Option<String>, pub due_text: Option<String>, pub source: String }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingExportSnapshot {
    pub id: String, pub title: String, pub created_at: String, pub captured_at: String,
    pub audio_available: bool, pub notes: ExportNotes, pub summary: Option<String>,
    pub summary_status: Option<String>, pub transcripts: Vec<ExportTranscript>, pub actions: Vec<ExportAction>,
}

/// One SQLite read snapshot pins title, Notes revision, selected transcript and
/// current summary together, including during atomic #14 selection/correction.
/// Audio availability is only a filesystem observation, never a retention claim.
pub async fn export_snapshot(pool: &SqlitePool, id: &str, expected_notes_revision: i64) -> Result<MeetingExportSnapshot, String> {
    crate::post_call::store::projection::recover(pool,Some(id)).await?;
    async fn read(pool: &SqlitePool, id: &str) -> Result<MeetingExportSnapshot, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let row = sqlx::query("SELECT title,created_at,folder_path FROM meetings WHERE id=?").bind(id).fetch_one(&mut *tx).await?;
        let title = row.try_get("title")?;
        let created_at = row.try_get("created_at")?;
        let folder: Option<String> = row.try_get("folder_path")?;
        let notes = sqlx::query_as::<_,(Option<String>,Option<String>,i64)>("SELECT notes_markdown,notes_json,revision FROM meeting_notes WHERE meeting_id=?").bind(id).fetch_optional(&mut *tx).await?;
        let (markdown, json, revision) = notes.unwrap_or((None,None,0));
        let json = json.map(|raw| serde_json::from_str(&raw)).transpose().map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let summary = sqlx::query_as::<_,(Option<String>,String)>("SELECT result,status FROM current_meeting_summaries WHERE meeting_id=?").bind(id).fetch_optional(&mut *tx).await?;
        let rows = sqlx::query("SELECT id,transcript,speaker,audio_start_time FROM transcripts WHERE meeting_id=? ORDER BY COALESCE(audio_start_time,0),timestamp,id").bind(id).fetch_all(&mut *tx).await?;
        let transcripts = rows.into_iter().map(|r| Ok(ExportTranscript { id:r.try_get("id")?, text:r.try_get("transcript")?, speaker:r.try_get("speaker")?, audio_start_time:r.try_get("audio_start_time")? })).collect::<Result<Vec<_>,sqlx::Error>>()?;
        let rows = sqlx::query("SELECT a.text,a.done,COALESCE(p.display_name,a.owner_label) AS owner,a.due_text,a.source FROM action_items a LEFT JOIN people p ON p.id=a.person_id WHERE a.meeting_id=? ORDER BY a.position,a.id").bind(id).fetch_all(&mut *tx).await?;
        let actions = rows.into_iter().map(|r| Ok(ExportAction { text:r.try_get("text")?, done:r.try_get("done")?, owner:r.try_get("owner")?, due_text:r.try_get("due_text")?, source:r.try_get("source")? })).collect::<Result<Vec<_>,sqlx::Error>>()?;
        tx.commit().await?;
        Ok(MeetingExportSnapshot { id:id.into(),title,created_at,captured_at:chrono::Utc::now().to_rfc3339(),audio_available:audio_available(folder.as_deref()),notes:ExportNotes {source:"user_notes",markdown:markdown.unwrap_or_default(),json,revision},summary:summary.as_ref().and_then(|v| v.0.clone()),summary_status:summary.map(|v| v.1),transcripts,actions })
    }
    let snapshot = read(pool, id).await.map_err(|e| format!("Could not capture meeting export: {e}"))?;
    if snapshot.notes.revision != expected_notes_revision { return Err("Notes changed while export was being captured. Reopen Notes and retry.".into()) }
    Ok(snapshot)
}
