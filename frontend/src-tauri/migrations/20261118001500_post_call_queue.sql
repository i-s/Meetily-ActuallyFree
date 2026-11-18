-- One automatic pipeline per saved meeting. Explicit refresh reuses this job;
-- publication is a separate immutable artifact, never the editable summary/Notes.
CREATE TABLE post_call_jobs (
    meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','deferred','waiting','cancelled','complete')),
    stage TEXT NOT NULL DEFAULT 'local' CHECK(stage IN ('local','summary')),
    epoch INTEGER NOT NULL DEFAULT 0,
    candidate_id TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE UNIQUE INDEX post_call_one_running ON post_call_jobs(status) WHERE status = 'running';
CREATE TABLE post_call_results (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL,
    transcript_revision INTEGER NOT NULL,
    notes_revision INTEGER NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(meeting_id, version_id, transcript_revision, notes_revision)
);
