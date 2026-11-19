-- Notes have their own revision clock; ASR and summaries never write these rows.
ALTER TABLE meeting_notes ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
CREATE TABLE recording_notes (
    folder_path TEXT PRIMARY KEY NOT NULL,
    notes_markdown TEXT,
    notes_json TEXT,
    revision INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
