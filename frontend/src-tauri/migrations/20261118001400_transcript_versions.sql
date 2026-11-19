-- Canonical transcripts remain the selected, editable projection used by existing
-- readers. Snapshots are immutable; edits are checkpointed before selection changes.
CREATE TABLE transcript_versions (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('live', 'refined', 'corrections')),
    status TEXT NOT NULL CHECK(status IN ('running', 'ready', 'failed', 'cancelled', 'interrupted')),
    created_at TEXT NOT NULL,
    parent_id TEXT,
    settings_json TEXT NOT NULL DEFAULT '{}',
    segments_json TEXT,
    error TEXT,
    summary_json TEXT,
    CHECK(status != 'ready' OR segments_json IS NOT NULL)
);
CREATE INDEX transcript_versions_meeting ON transcript_versions(meeting_id, created_at);
CREATE TABLE transcript_selection (
    meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL REFERENCES transcript_versions(id),
    revision INTEGER NOT NULL DEFAULT 0
);
CREATE TRIGGER transcript_revision_insert AFTER INSERT ON transcripts BEGIN
    UPDATE transcript_selection SET revision = revision + 1 WHERE meeting_id = NEW.meeting_id;
END;
CREATE TRIGGER transcript_revision_update AFTER UPDATE ON transcripts BEGIN
    UPDATE transcript_selection SET revision = revision + 1 WHERE meeting_id = NEW.meeting_id OR meeting_id = OLD.meeting_id;
END;
CREATE TRIGGER transcript_revision_delete AFTER DELETE ON transcripts BEGIN
    UPDATE transcript_selection SET revision = revision + 1 WHERE meeting_id = OLD.meeting_id;
END;
