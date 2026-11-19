-- Content/timestamps cannot identify a summary job: an UPSERT may regenerate
-- identical text in the same row. Every committed row write gets fresh ownership.
ALTER TABLE summary_processes ADD COLUMN action_sync_token TEXT NOT NULL DEFAULT '';
UPDATE summary_processes SET action_sync_token = hex(randomblob(16));
CREATE TRIGGER summary_action_sync_insert AFTER INSERT ON summary_processes BEGIN
    UPDATE summary_processes SET action_sync_token = hex(randomblob(16)) WHERE meeting_id = NEW.meeting_id;
END;
CREATE TRIGGER summary_action_sync_update AFTER UPDATE ON summary_processes
WHEN NEW.action_sync_token = OLD.action_sync_token BEGIN
    UPDATE summary_processes SET action_sync_token = hex(randomblob(16)) WHERE meeting_id = NEW.meeting_id;
END;
