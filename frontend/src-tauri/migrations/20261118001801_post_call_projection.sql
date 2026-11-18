-- A projection is disposable; the immutable result and applied receipt are not.
CREATE TABLE post_call_projection_attempts (
 meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
 epoch INTEGER NOT NULL,
 summary_token TEXT,
 eligible INTEGER NOT NULL,
 PRIMARY KEY(meeting_id,epoch)
);
CREATE TABLE post_call_projections (
 meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
 version_id TEXT NOT NULL,
 transcript_revision INTEGER NOT NULL,
 notes_revision INTEGER NOT NULL,
 vocabulary_revision INTEGER NOT NULL,
 job_epoch INTEGER NOT NULL,
 summary_token TEXT NOT NULL
);
CREATE TABLE post_call_projection_receipts (
 meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
 version_id TEXT NOT NULL,
 transcript_revision INTEGER NOT NULL,
 notes_revision INTEGER NOT NULL,
 vocabulary_revision INTEGER NOT NULL,
 job_epoch INTEGER NOT NULL,
 PRIMARY KEY(meeting_id,version_id,transcript_revision,notes_revision,vocabulary_revision)
);
-- Keep the underlying row for manual draft saves and #14's archive-before-clear
-- ordering. Current readers hide obsolete untouched projections via the view;
-- Notes changes do not physically delete a row an editor may still be saving.
-- Shared readers hide an untouched generated projection after a raw transcript
-- edit without destroying the checkpoint needed by explicit version selection.
CREATE VIEW current_meeting_summaries AS
 SELECT s.* FROM summary_processes s WHERE NOT EXISTS (
 SELECT 1 FROM post_call_projections p WHERE p.meeting_id=s.meeting_id
 AND s.user_edited=0 AND CASE WHEN json_valid(s.result) THEN json_extract(s.result,'$.post_call.source')='post_call_results' ELSE 0 END
 AND (NOT EXISTS(SELECT 1 FROM transcript_selection t WHERE t.meeting_id=p.meeting_id AND t.version_id=p.version_id AND t.revision=p.transcript_revision)
 OR COALESCE((SELECT revision FROM meeting_notes WHERE meeting_id=p.meeting_id),0)!=p.notes_revision));
