-- Do not rewrite 01600: it may already be recorded in _sqlx_migrations.
-- Its triggers missed REPLACE-to-empty and canonical ID moves. A stale proof
-- can survive an out-and-back move even when the final text still matches, so
-- membership alone cannot recover historical approval safely. Keep ASR terms
-- untouched and require explicit review/confirmation of existing proofs once.
UPDATE vocabulary_approval
SET revision = revision + 1, approved_terms = ''
WHERE id = 1 AND approved_terms != '';

DROP TRIGGER vocabulary_global_insert;
DROP TRIGGER vocabulary_global_update;
DROP TRIGGER vocabulary_global_delete;

-- REPLACE's implicit DELETE may not fire when recursive_triggers is OFF.
-- Its INSERT always fires, including replacement by NULL/empty or equal text.
CREATE TRIGGER vocabulary_global_insert AFTER INSERT ON transcript_settings
WHEN NEW.id = '1'
BEGIN
    UPDATE vocabulary_approval SET revision = revision + 1, approved_terms = '' WHERE id = 1;
END;

-- Moving a row into/out of the canonical slot changes dictionary ownership,
-- even without assigning whisperVocabulary. IS NOT also covers NULL changes.
-- Unrelated rows and actual no-op canonical updates leave the snapshot alone.
CREATE TRIGGER vocabulary_global_update AFTER UPDATE OF id, whisperVocabulary ON transcript_settings
WHEN (OLD.id = '1' OR NEW.id = '1')
    AND (NEW.id IS NOT OLD.id OR NEW.whisperVocabulary IS NOT OLD.whisperVocabulary)
BEGIN
    UPDATE vocabulary_approval SET revision = revision + 1, approved_terms = '' WHERE id = 1;
END;

CREATE TRIGGER vocabulary_global_delete AFTER DELETE ON transcript_settings
WHEN OLD.id = '1'
BEGIN
    UPDATE vocabulary_approval SET revision = revision + 1, approved_terms = '' WHERE id = 1;
END;
