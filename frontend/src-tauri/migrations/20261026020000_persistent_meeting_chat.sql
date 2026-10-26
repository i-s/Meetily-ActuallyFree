CREATE TABLE meeting_chat_turns (
    id TEXT PRIMARY KEY NOT NULL,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    question TEXT NOT NULL,
    answer TEXT,
    status TEXT NOT NULL CHECK(status IN ('pending','completed','error','interrupted')),
    error TEXT,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    context_json TEXT NOT NULL
);
CREATE INDEX meeting_chat_history ON meeting_chat_turns(meeting_id, created_at);
CREATE UNIQUE INDEX meeting_chat_one_pending ON meeting_chat_turns(meeting_id) WHERE status = 'pending';

-- Match explicit deletion in the existing repositories on legacy connections
-- that may have foreign_keys disabled. Completion only updates an existing row.
CREATE TRIGGER meeting_chat_delete AFTER DELETE ON meetings BEGIN
    DELETE FROM meeting_chat_turns WHERE meeting_id = OLD.id;
END;

CREATE TABLE ask_ai_context_settings (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    mode TEXT NOT NULL CHECK(mode IN ('recent','all')),
    max_chars INTEGER NOT NULL CHECK(max_chars > 0)
);
INSERT INTO ask_ai_context_settings(id, mode, max_chars) VALUES (1, 'recent', 10000);
