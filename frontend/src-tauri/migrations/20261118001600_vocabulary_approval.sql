-- Existing entries have no durable confirmation proof. Preserve their local
-- ASR behavior, but require review/Save or explicit Remember before Codex use.
CREATE TABLE vocabulary_approval (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    approved_terms TEXT NOT NULL DEFAULT ''
);
INSERT INTO vocabulary_approval (id) VALUES (1);

-- Legacy/native SQL writers cannot silently promote stored text to approved
-- model context. The explicit confirmation transaction restores only its proof.
CREATE TRIGGER vocabulary_global_insert AFTER INSERT ON transcript_settings
WHEN NEW.id = '1' AND COALESCE(NEW.whisperVocabulary, '') != ''
BEGIN
    UPDATE vocabulary_approval SET revision = revision + 1, approved_terms = '' WHERE id = 1;
END;
CREATE TRIGGER vocabulary_global_update AFTER UPDATE OF whisperVocabulary ON transcript_settings
WHEN NEW.id = '1' AND COALESCE(NEW.whisperVocabulary, '') != COALESCE(OLD.whisperVocabulary, '')
BEGIN
    UPDATE vocabulary_approval SET revision = revision + 1, approved_terms = '' WHERE id = 1;
END;
CREATE TRIGGER vocabulary_global_delete AFTER DELETE ON transcript_settings
WHEN OLD.id = '1'
BEGIN
    UPDATE vocabulary_approval SET revision = revision + 1, approved_terms = '' WHERE id = 1;
END;
