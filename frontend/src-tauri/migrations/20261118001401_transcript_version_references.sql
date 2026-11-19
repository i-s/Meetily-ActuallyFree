-- Snapshot curated label/contact associations independently of display names.
ALTER TABLE transcript_versions ADD COLUMN speaker_links_json TEXT;
-- Tasks retain their original transcript/version while their current navigation
-- anchor follows the selected projection. Never delete a task on resegmentation.
CREATE TABLE action_item_transcript_sources (
    action_id TEXT PRIMARY KEY REFERENCES action_items(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL REFERENCES transcript_versions(id) ON DELETE CASCADE,
    transcript_id TEXT,
    audio_time REAL
);
