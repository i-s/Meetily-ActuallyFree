//! The persistence implementation is included unchanged. Only its transport DTO
//! is mirrored here so SQLite tests need neither Tauri's GTK nor ASR libraries.
mod api {
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    pub struct TranscriptSegment {
        pub id: String,
        pub text: String,
        pub timestamp: String,
        pub audio_start_time: Option<f64>,
        pub audio_end_time: Option<f64>,
        pub duration: Option<f64>,
        pub speaker: Option<String>,
    }
}
#[path = "../../../frontend/src-tauri/src/database/transcript_versions.rs"]
mod transcript_versions;

#[path = "../../../frontend/src-tauri/src/database/repositories/speaker_identity.rs"]
pub mod speaker_identity;
mod database { pub mod repositories { pub use crate::speaker_identity; } }

mod config { pub const DEFAULT_PARAKEET_MODEL: &str = "synthetic"; }
#[path = "../../../frontend/src-tauri/src/database/repositories/vocabulary.rs"]
mod vocabulary;
#[cfg(test)]
mod tests {
    use super::vocabulary::VocabularyRepository as V;
    use sqlx::sqlite::SqlitePoolOptions;
    pub(super) async fn pool() -> sqlx::SqlitePool {
        let p = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE transcript_settings(id TEXT PRIMARY KEY, provider TEXT, model TEXT, whisperVocabulary TEXT)").execute(&p).await.unwrap();
        sqlx::raw_sql(include_str!("../../../frontend/src-tauri/migrations/20261118001600_vocabulary_approval.sql")).execute(&p).await.unwrap();
        p
    }
    async fn replace(p:&sqlx::SqlitePool, expected:&str, raw:&str)->Result<Option<String>,String> {
        let revision=V::global_state(p).await.map_err(|e|e.to_string())?.revision;
        V::replace_global_at_revision(p,expected,revision,raw).await
    }
    #[tokio::test]
    async fn confirmed_mixed_terms_retry_and_stale_editor_cannot_erase_addition() {
        let p = pool().await;
        assert_eq!(V::confirm_global(&p, " Ёлка API ").await.unwrap().as_deref(), Some("Ёлка API"));
        assert_eq!(V::confirm_global(&p, "ёлка api").await.unwrap().as_deref(), Some("Ёлка API"));
        assert!(replace(&p, "", "Wrong overwrite").await.is_err());
        assert_eq!(V::get_global(&p).await.unwrap().as_deref(), Some("Ёлка API"));
        replace(&p, "Ёлка API", "Елена\nOpenAPI\nSRE").await.unwrap();
        replace(&p, "Елена\nOpenAPI\nSRE", "OpenAPI\nSRE").await.unwrap();
        assert_eq!(V::get_global(&p).await.unwrap().as_deref(), Some("OpenAPI\nSRE"));
    }
    #[tokio::test]
    async fn concurrent_confirmations_keep_both_and_invalid_or_full_additions_do_not_write() {
        let p = pool().await;
        let (a,b) = tokio::join!(V::confirm_global(&p,"Ирина"), V::confirm_global(&p,"Kubernetes"));
        a.unwrap(); b.unwrap();
        assert_eq!(V::get_global(&p).await.unwrap().as_deref(), Some("Ирина\nKubernetes"));
        for raw in ["", "one,two", "one\ntwo", "bad\0term", "bad\tterm"] { assert!(V::confirm_global(&p,raw).await.is_err(), "{raw:?}"); }
        let full = "Ж".repeat(1000);
        replace(&p, "Ирина\nKubernetes", &full).await.unwrap();
        assert!(V::confirm_global(&p,"overflow").await.is_err());
        assert_eq!(V::get_global(&p).await.unwrap(), Some(full.clone()));
        assert_eq!(V::confirm_global(&p,&full).await.unwrap(), Some(full));
    }
    #[tokio::test]
    async fn legacy_entries_need_review_and_confirming_one_never_approves_the_rest() {
        let p = pool().await;
        sqlx::query("INSERT INTO transcript_settings VALUES ('1','parakeet','synthetic','LegacyName')").execute(&p).await.unwrap();
        let old = V::approved_global(&p).await.unwrap();
        assert_eq!(old.terms, "");
        V::confirm_global(&p, "OpenAPI").await.unwrap();
        let confirmed = V::approved_global(&p).await.unwrap();
        assert_eq!(confirmed.terms, "OpenAPI");
        assert!(confirmed.revision > old.revision);
        V::confirm_global(&p, "openapi").await.unwrap();
        assert_eq!(V::approved_global(&p).await.unwrap().revision, confirmed.revision);
        replace(&p, "LegacyName\nOpenAPI", "LegacyName\nOpenAPI").await.unwrap();
        let reviewed = V::approved_global(&p).await.unwrap();
        assert_eq!(reviewed.terms, "LegacyName\nOpenAPI");
        assert!(reviewed.revision > confirmed.revision);
        replace(&p, "LegacyName\nOpenAPI", "OpenAPI").await.unwrap();
        assert_eq!(V::approved_global(&p).await.unwrap().terms, "OpenAPI");
        // An unversioned legacy writer must invalidate proof, not approve text.
        V::save_global(&p, Some("ModelSuggestion")).await.unwrap();
        assert_eq!(V::approved_global(&p).await.unwrap().terms, "");
    }

    #[tokio::test]
    async fn first_explicit_confirmation_controls_legacy_case_then_retries_preserve_it() {
        let p=pool().await;
        V::save_global(&p,Some("openapi\nLegacy")).await.unwrap();
        V::confirm_global(&p,"OpenAPI").await.unwrap();
        assert_eq!(V::get_global(&p).await.unwrap().as_deref(),Some("OpenAPI\nLegacy"));
        assert_eq!(V::approved_global(&p).await.unwrap().terms,"OpenAPI");
        V::confirm_global(&p,"OPENAPI").await.unwrap();
        assert_eq!(V::approved_global(&p).await.unwrap().terms,"OpenAPI");
    }
    #[tokio::test]
    async fn review_revision_cas_rejects_same_text_after_new_approval_and_control_chars() {
        let p = pool().await;
        sqlx::query("INSERT INTO transcript_settings VALUES ('1','parakeet','synthetic','Legacy')").execute(&p).await.unwrap();
        let stale = V::global_state(&p).await.unwrap();
        V::confirm_global(&p, "Legacy").await.unwrap();
        assert!(V::replace_global_at_revision(&p, "Legacy", stale.revision, "").await.is_err());
        assert!(V::normalize("bad\tterm").is_err());
    }

    #[tokio::test]
    async fn vocabulary_survives_reopen_with_same_revision_and_full_unicode_spelling() {
        let path=std::env::temp_dir().join(format!("meetily-vocabulary-{}.sqlite",uuid::Uuid::new_v4()));
        let opts=sqlx::sqlite::SqliteConnectOptions::new().filename(&path).create_if_missing(true);
        let p=SqlitePoolOptions::new().max_connections(1).connect_with(opts.clone()).await.unwrap();
        sqlx::query("CREATE TABLE transcript_settings(id TEXT PRIMARY KEY, provider TEXT, model TEXT, whisperVocabulary TEXT)").execute(&p).await.unwrap();
        sqlx::raw_sql(include_str!("../../../frontend/src-tauri/migrations/20261118001600_vocabulary_approval.sql")).execute(&p).await.unwrap();
        V::confirm_global(&p,"Елена OpenAPI").await.unwrap();
        let saved=V::approved_global(&p).await.unwrap(); p.close().await;
        let reopened=SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
        let restored=V::approved_global(&reopened).await.unwrap();
        assert_eq!(restored.terms,"Елена OpenAPI"); assert_eq!(restored.revision,saved.revision);
        assert_eq!(V::get_global(&reopened).await.unwrap().as_deref(),Some("Елена OpenAPI"));
        reopened.close().await; std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn confirmation_is_independent_of_legacy_meeting_edits_links_notes_and_version_cas() {
        let p=pool().await;
        sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY, folder_path TEXT); CREATE TABLE transcripts(id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, timestamp TEXT, audio_start_time REAL, audio_end_time REAL, duration REAL, speaker TEXT); CREATE TABLE people(id TEXT PRIMARY KEY, display_name TEXT, normalized_name TEXT, notes TEXT, created_at TEXT, updated_at TEXT); CREATE TABLE action_items(id TEXT PRIMARY KEY, meeting_id TEXT, transcript_id TEXT, audio_time REAL, text TEXT, done INTEGER, edited INTEGER); CREATE TABLE person_speakers(person_id TEXT, meeting_id TEXT, speaker_label TEXT, UNIQUE(meeting_id, speaker_label)); CREATE TABLE transcript_chunks(meeting_id TEXT); CREATE TABLE summary_processes(meeting_id TEXT, status TEXT, result TEXT, result_backup TEXT, user_edited INTEGER); INSERT INTO meetings VALUES ('m', NULL); INSERT INTO meetings VALUES ('other', NULL); INSERT INTO transcripts VALUES ('live-row', 'm', 'RU/EN original API', '2026-11-18T10:00:01Z', 1, 2, 1, 'You');")
.execute(&p).await.unwrap();
        sqlx::raw_sql(include_str!("../../../frontend/src-tauri/migrations/20261118001400_transcript_versions.sql")).execute(&p).await.unwrap();
        sqlx::raw_sql(include_str!("../../../frontend/src-tauri/migrations/20261118001401_transcript_version_references.sql")).execute(&p).await.unwrap();
        sqlx::raw_sql("INSERT INTO people VALUES ('person','Елена','елена','private','now','now'); INSERT INTO person_speakers VALUES ('person','m','You'); CREATE TABLE user_notes(text TEXT); INSERT INTO user_notes VALUES ('personal thought, not speech');").execute(&p).await.unwrap();
        let old=crate::transcript_versions::selected(&p,"m").await.unwrap();
        crate::transcript_versions::correct_text(&p,"m","live-row","OpenAPI / Ёлка API",old.revision).await.unwrap();
        let corrected=crate::transcript_versions::selected(&p,"m").await.unwrap();
        // Declining Remember means no vocabulary operation at all.
        assert_eq!(V::get_global(&p).await.unwrap(),None);
        assert_eq!(corrected.segments[0].text,"OpenAPI / Ёлка API");
        sqlx::query("UPDATE transcripts SET speaker='Елена' WHERE id='live-row'").execute(&p).await.unwrap();
        assert!(crate::transcript_versions::correct_text(&p,"m","live-row","stale",corrected.revision).await.is_err());
        let edited=crate::transcript_versions::selected(&p,"m").await.unwrap();
        V::confirm_global(&p,"OpenAPI").await.unwrap(); V::confirm_global(&p,"openapi").await.unwrap();
        let after=crate::transcript_versions::selected(&p,"m").await.unwrap();
        assert_eq!(after.revision,edited.revision);
        assert_eq!(after.segments[0].text,"OpenAPI / Ёлка API"); assert_eq!(after.segments[0].speaker.as_deref(),Some("Елена"));
        assert_eq!(after.segments[0].audio_start_time,Some(1.0)); assert_eq!(after.segments[0].id,"live-row");
        let link:String=sqlx::query_scalar("SELECT person_id FROM person_speakers WHERE meeting_id='m'").fetch_one(&p).await.unwrap(); assert_eq!(link,"person");
        let note:String=sqlx::query_scalar("SELECT text FROM user_notes").fetch_one(&p).await.unwrap(); assert_eq!(note,"personal thought, not speech");
        let versions=crate::transcript_versions::list(&p,"m").await.unwrap();
        let versions=serde_json::to_value(versions).unwrap();
        let history=versions["versions"].as_array().unwrap().iter().find(|v|v["kind"]=="corrections").unwrap();
        let rows=crate::transcript_versions::segments(&p,"m",history["id"].as_str().unwrap()).await.unwrap();
        assert_eq!(rows[0].text,"RU/EN original API");
        assert!(crate::transcript_versions::selected(&p,"other").await.unwrap().segments.is_empty());
    }

}

#[cfg(test)]
mod asr_consumers;
#[cfg(test)]
fn get_language_preference_internal()->Option<String>{Some("auto".into())}
#[cfg(test)]
mod audio {pub mod audio_processing {pub fn resample_audio(_: &[f32],_:u32,_:u32)->Vec<f32>{unreachable!("fixture is already 16kHz")}}}

#[cfg(test)]
extern crate self as tauri;
#[cfg(test)]
pub struct State<'a,T>(pub &'a T);
#[cfg(test)]
impl<'a,T> std::ops::Deref for State<'a,T>{type Target=T;fn deref(&self)->&T{self.0}}
#[cfg(test)]
mod commands {
    use crate::vocabulary::VocabularyRepository;
    pub struct AppState {pub db_manager:DbManager}
    pub struct DbManager(pub sqlx::SqlitePool);
    impl DbManager {pub fn pool(&self)->&sqlx::SqlitePool{&self.0}}
    include!(concat!(env!("OUT_DIR"),"/commands.rs"));
    #[tokio::test]
    async fn normal_native_commands_only_confirm_explicit_terms_and_use_loaded_value_and_revision() {
        let p=crate::tests::pool().await;
        let state=AppState{db_manager:DbManager(p)};
        let loaded=api_get_whisper_vocabulary(crate::State(&state),None).await.unwrap();
        assert_eq!(loaded.global,"");
        api_confirm_vocabulary_term(crate::State(&state),"OpenAPI".into()).await.unwrap();
        assert!(api_save_global_whisper_vocabulary(crate::State(&state),"stale overwrite".into(),loaded.global,loaded.vocabulary_revision).await.is_err());
        let view=api_get_whisper_vocabulary(crate::State(&state),None).await.unwrap();
        let json=serde_json::to_value(&view).unwrap();
        assert_eq!(json["approvedGlobal"],"OpenAPI");
        assert!(json["vocabularyRevision"].as_i64().unwrap()>0);
        api_save_global_whisper_vocabulary(crate::State(&state),"".into(),view.global,view.vocabulary_revision).await.unwrap();
        let empty=api_get_whisper_vocabulary(crate::State(&state),None).await.unwrap();
        assert_eq!(empty.global,""); assert_eq!(empty.approved_global,"");
    }
}
