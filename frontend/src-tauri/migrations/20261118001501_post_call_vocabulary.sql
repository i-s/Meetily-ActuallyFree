-- Bind spelling guidance to an executing attempt, never to mutable global text.
ALTER TABLE post_call_jobs ADD COLUMN vocabulary_revision INTEGER;

-- Preserve pre-integration artifacts under an explicit unbound legacy sentinel.
-- They must not satisfy a cache lookup for an approved (including empty) snapshot.
ALTER TABLE post_call_results RENAME TO post_call_results_before_vocabulary;
CREATE TABLE post_call_results (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL,
    transcript_revision INTEGER NOT NULL,
    notes_revision INTEGER NOT NULL,
    vocabulary_revision INTEGER NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(meeting_id, version_id, transcript_revision, notes_revision, vocabulary_revision)
);
INSERT INTO post_call_results(meeting_id,version_id,transcript_revision,notes_revision,vocabulary_revision,result_json,created_at)
    SELECT meeting_id,version_id,transcript_revision,notes_revision,-1,result_json,created_at
    FROM post_call_results_before_vocabulary ORDER BY rowid;
DROP TABLE post_call_results_before_vocabulary;
