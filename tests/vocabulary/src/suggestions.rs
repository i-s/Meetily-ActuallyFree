//! Regression targets: accidental ensure/write on scan, stale approval, or
//! suggestion confirmation bypassing the existing approval transaction.
use crate::{commands::*, vocabulary::VocabularyRepository as V};
use sqlx::SqlitePool;

async fn fixture() -> AppState {
    let p = crate::tests::pool().await;
    sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY, folder_path TEXT); CREATE TABLE transcripts(id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, timestamp TEXT, audio_start_time REAL, audio_end_time REAL, duration REAL, speaker TEXT); CREATE TABLE person_speakers(person_id TEXT, meeting_id TEXT, speaker_label TEXT); CREATE TABLE summary_processes(meeting_id TEXT, status TEXT, result TEXT, result_backup TEXT, user_edited INTEGER); CREATE TABLE action_items(id TEXT PRIMARY KEY, meeting_id TEXT, transcript_id TEXT); INSERT INTO meetings VALUES ('m',NULL),('other',NULL); INSERT INTO transcripts VALUES ('row','m','Обсудили OpenAPI, СУБД и ЁлкаAPI2.', 'now',1,2,1,'You'),('foreign','other','SecretAPI','now',2,3,1,'Speaker 1'); CREATE TABLE user_notes(text TEXT); INSERT INTO user_notes VALUES ('Notes are not speech');").execute(&p).await.unwrap();
    sqlx::raw_sql(include_str!("../../../frontend/src-tauri/migrations/20261118001400_transcript_versions.sql")).execute(&p).await.unwrap();
    sqlx::raw_sql(include_str!("../../../frontend/src-tauri/migrations/20261118001401_transcript_version_references.sql")).execute(&p).await.unwrap();
    AppState { db_manager: DbManager(p) }
}
async fn changes(p: &SqlitePool) -> i64 { sqlx::query_scalar("SELECT total_changes()").fetch_one(p).await.unwrap() }
async fn confirm(s: &AppState, version: &str, revision: i64, row: &str, term: &str) -> Result<String,String> {
    api_confirm_vocabulary_suggestion(crate::State(s),"m".into(),version.into(),revision,row.into(),term.into()).await
}

#[tokio::test]
async fn scan_is_read_only_even_when_legacy_selection_is_missing() {
    let s=fixture().await; let p=s.db_manager.pool();
    let before=changes(p).await;
    assert!(api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.is_err());
    assert_eq!(changes(p).await,before);
    let selected=crate::transcript_versions::selected(p,"m").await.unwrap();
    let before=changes(p).await;
    let view=api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap();
    assert_eq!(view.version_id,selected.version_id); assert_eq!(view.revision,selected.revision);
    assert_eq!(view.segments.len(),1); assert_eq!(view.segments[0].text,"Обсудили OpenAPI, СУБД и ЁлкаAPI2.");
    assert_eq!(changes(p).await,before);
    assert_eq!(V::approved_global(p).await.unwrap().terms,"");
}

#[tokio::test]
async fn explicit_suggestion_confirmation_reuses_approval_and_does_not_change_projection_or_notes() {
    let s=fixture().await; let p=s.db_manager.pool();
    let c=crate::transcript_versions::selected(p,"m").await.unwrap();
    let before=serde_json::to_value(api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap()).unwrap();
    for term in ["OpenAPI","СУБД","ЁлкаAPI2"] { confirm(&s,&c.version_id,c.revision,"row",term).await.unwrap(); }
    let approved=V::approved_global(p).await.unwrap();
    assert_eq!(approved.terms,"OpenAPI\nСУБД\nЁлкаAPI2");
    // Exact source casing still cannot replace an already approved spelling.
    V::confirm_global(p,"openapi").await.unwrap();
    assert_eq!(V::approved_global(p).await.unwrap().revision,approved.revision);
    assert_eq!(serde_json::to_value(api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap()).unwrap(),before);
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT text FROM user_notes").fetch_one(p).await.unwrap(),"Notes are not speech");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM transcript_versions").fetch_one(p).await.unwrap(),1);
}

#[tokio::test]
async fn stale_or_fabricated_source_never_changes_dictionary_or_approval_revision() {
    let s=fixture().await; let p=s.db_manager.pool();
    let c=crate::transcript_versions::selected(p,"m").await.unwrap();
    for (v,r,row,term) in [("wrong",c.revision,"row","OpenAPI"),(&*c.version_id,c.revision+1,"row","OpenAPI"),(&*c.version_id,c.revision,"foreign","SecretAPI"),(&*c.version_id,c.revision,"row","GuessAPI"),(&*c.version_id,c.revision,"row","openapi"),(&*c.version_id,c.revision,"row","API"),(&*c.version_id,c.revision,"row","")] {
        let before=changes(p).await;
        assert!(confirm(&s,v,r,row,term).await.is_err(),"{v}/{row}/{term}");
        assert_eq!(changes(p).await,before);
        assert_eq!(V::approved_global(p).await.unwrap().revision,0);
    }
    sqlx::query("UPDATE transcripts SET transcript='LaterAPI' WHERE id='row'").execute(p).await.unwrap();
    assert!(confirm(&s,&c.version_id,c.revision,"row","OpenAPI").await.is_err());
    assert_eq!(V::get_global(p).await.unwrap(),None);
}

#[tokio::test]
async fn concurrent_confirmation_is_idempotent_and_full_store_failure_is_atomic() {
    let s=fixture().await; let p=s.db_manager.pool();
    let c=crate::transcript_versions::selected(p,"m").await.unwrap();
    let (a,b)=tokio::join!(confirm(&s,&c.version_id,c.revision,"row","OpenAPI"),confirm(&s,&c.version_id,c.revision,"row","OpenAPI"));
    a.unwrap();b.unwrap();
    assert_eq!(V::approved_global(p).await.unwrap().terms,"OpenAPI");
    assert_eq!(V::approved_global(p).await.unwrap().revision,1);
    let st=V::global_state(p).await.unwrap();
    V::replace_global_at_revision(p,&st.global,st.revision,&"Ж".repeat(1000)).await.unwrap();
    let before=V::approved_global(p).await.unwrap();
    assert!(confirm(&s,&c.version_id,c.revision,"row","СУБД").await.is_err());
    let after=V::approved_global(p).await.unwrap();
    assert_eq!(before.terms,after.terms); assert_eq!(before.revision,after.revision);
}

#[tokio::test]
async fn context_bounds_rows_and_large_unicode_lines_without_settings_or_partial_tokens() {
    let s=fixture().await; let p=s.db_manager.pool();
    let c=crate::transcript_versions::selected(p,"m").await.unwrap();
    sqlx::query("UPDATE transcript_versions SET settings_json=? WHERE id=?")
        .bind(format!("{{\"source_map\":\"{}\"}}", "metadata".repeat(100000))).bind(&c.version_id).execute(p).await.unwrap();
    sqlx::query("UPDATE transcripts SET transcript=? WHERE id='row'")
        .bind(format!("{}OpenAPI", "Я😀 ".repeat(33332))).execute(p).await.unwrap();
    let before=changes(p).await;
    let context=api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap();
    let json=serde_json::to_value(&context).unwrap();
    assert!(json.get("settings_json").is_none()); assert!(context.truncated);
    assert!(context.segments[0].text.chars().count() <= 100000);
    assert!(!context.segments[0].text.ends_with("Open"));
    assert!(!context.segments[0].text.contains("OpenAPI"));
    assert_eq!(changes(p).await,before);
    sqlx::query("UPDATE transcripts SET transcript='ordinary' WHERE id='row'").execute(p).await.unwrap();
    for n in 0..2001 { sqlx::query("INSERT INTO transcripts VALUES (?, 'm','ordinary','now',3,4,1,'You')").bind(format!("extra-{n:04}")).execute(p).await.unwrap(); }
    let before=changes(p).await;
    let context=api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap();
    assert_eq!(context.segments.len(),2000); assert!(context.truncated); assert_eq!(changes(p).await,before);
}

#[tokio::test]
async fn concurrent_preapproved_case_wins_and_version_only_switch_rejects_same_source() {
    let s=fixture().await; let p=s.db_manager.pool();
    let c=crate::transcript_versions::selected(p,"m").await.unwrap();
    V::confirm_global(p,"openapi").await.unwrap();
    let revision=V::approved_global(p).await.unwrap().revision;
    confirm(&s,&c.version_id,c.revision,"row","OpenAPI").await.unwrap();
    assert_eq!(V::approved_global(p).await.unwrap().terms,"openapi");
    assert_eq!(V::approved_global(p).await.unwrap().revision,revision);
    sqlx::query("INSERT INTO transcript_versions(id,meeting_id,kind,status,created_at,segments_json) VALUES ('another','m','refined','ready','now','[]')").execute(p).await.unwrap();
    sqlx::query("UPDATE transcript_selection SET version_id='another' WHERE meeting_id='m'").execute(p).await.unwrap();
    assert!(confirm(&s,&c.version_id,c.revision,"row","СУБД").await.is_err());
    assert_eq!(V::approved_global(p).await.unwrap().terms,"openapi");
}

#[tokio::test]
async fn suggestion_proof_does_not_read_or_approve_an_unbounded_row_suffix() {
    let s=fixture().await; let p=s.db_manager.pool();
    crate::transcript_versions::selected(p,"m").await.unwrap();
    sqlx::query("UPDATE transcripts SET transcript=? WHERE id='row'")
        .bind(format!("OpenAPI {} HiddenAPI", "Я".repeat(200000))).execute(p).await.unwrap();
    let c=api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap();
    confirm(&s,&c.version_id,c.revision,"row","OpenAPI").await.unwrap();
    assert!(confirm(&s,&c.version_id,c.revision,"row","HiddenAPI").await.is_err());
    assert_eq!(V::approved_global(p).await.unwrap().terms,"OpenAPI");
}

#[tokio::test]
async fn r1_combining_marks_never_prove_fragments_but_exact_source_remains_confirmable() {
    let s=fixture().await; let p=s.db_manager.pool();
    crate::transcript_versions::selected(p,"m").await.unwrap();
    V::confirm_global(p,"ExistingAPI").await.unwrap();
    for (term,fragments) in [("е\u{0308}лкаAPI",vec!["е","лкаAPI"]),("E\u{0308}lkaAPI",vec!["E","lkaAPI"]),("OpenAPI\u{0301}Client",vec!["OpenAPI","Client"])] {
        sqlx::query("UPDATE transcripts SET transcript=? WHERE id='row'").bind(format!("{term} ControlAPI")).execute(p).await.unwrap();
        let c=api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap();
        let before=V::approved_global(p).await.unwrap();let global=V::get_global(p).await.unwrap();
        for fragment in fragments {
            let writes=changes(p).await;
            let result=confirm(&s,&c.version_id,c.revision,"row",fragment).await;
            let observed=V::approved_global(p).await.unwrap();
            assert!(result.is_err(),"fragment {fragment:?} in {term:?}; actual SQLite terms {:?}, revision {} -> {}",observed.terms,before.revision,observed.revision);
            assert_eq!(changes(p).await,writes);
            assert_eq!(V::get_global(p).await.unwrap(),global);
            let after=V::approved_global(p).await.unwrap();assert_eq!(after.terms,before.terms);assert_eq!(after.revision,before.revision);
        }
        confirm(&s,&c.version_id,c.revision,"row",term).await.unwrap();
        confirm(&s,&c.version_id,c.revision,"row","ControlAPI").await.unwrap();
    }
    assert_eq!(V::approved_global(p).await.unwrap().terms,"ExistingAPI\nе\u{0308}лкаAPI\nControlAPI\nE\u{0308}lkaAPI\nOpenAPI\u{0301}Client");
}

#[tokio::test]
async fn r1_sql_scalar_prefix_clipping_never_leaves_mark_separated_fragment_proof() {
    let s=fixture().await;let p=s.db_manager.pool();
    crate::transcript_versions::selected(p,"m").await.unwrap();
    V::confirm_global(p,"ExistingAPI").await.unwrap();
    for term in ["е\u{0308}лкаAPI","E\u{0308}lkaAPI","OpenAPI\u{0301}Client","OpenAPI\u{1D165}Client"] {
        let mark=term.chars().position(|c| ['\u{0308}','\u{0301}','\u{1D165}'].contains(&c)).unwrap();
        for offset in [0,1,2] {
            let prefix=" ".repeat(100000-mark-offset);
            sqlx::query("UPDATE transcripts SET transcript=? WHERE id='row'").bind(format!("{prefix}{term}")).execute(p).await.unwrap();
            let writes=changes(p).await;
            let c=api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap();
            assert!(c.truncated);assert!(c.segments[0].text_truncated);
            assert!(c.segments[0].text==prefix,"clipped token retained for {term:?}, offset {offset}");
            assert_eq!(changes(p).await,writes);
            let before=V::approved_global(p).await.unwrap();
            for fragment in ["е","лкаAPI","E","lkaAPI","OpenAPI","Client"] {
                assert!(confirm(&s,&c.version_id,c.revision,"row",fragment).await.is_err(),"{term:?}/{offset}/{fragment}");
                assert_eq!(changes(p).await,writes);
                let after=V::approved_global(p).await.unwrap();assert_eq!(after.terms,before.terms);assert_eq!(after.revision,before.revision);
            }
        }
    }
}

#[tokio::test]
async fn r1_all_mark_categories_are_continuations_without_changing_manual_confirmation() {
    let s=fixture().await;let p=s.db_manager.pool();
    crate::transcript_versions::selected(p,"m").await.unwrap();
    for mark in ['\u{0301}','\u{0903}','\u{20DD}','\u{1D165}'] {
        let term=format!("OpenAPI{mark}Client");
        sqlx::query("UPDATE transcripts SET transcript=? WHERE id='row'").bind(&term).execute(p).await.unwrap();
        let c=api_get_vocabulary_suggestion_context(crate::State(&s),"m".into()).await.unwrap();
        let before=V::approved_global(p).await.unwrap();
        assert!(confirm(&s,&c.version_id,c.revision,"row","OpenAPI").await.is_err());
        assert!(confirm(&s,&c.version_id,c.revision,"row","Client").await.is_err());
        let after=V::approved_global(p).await.unwrap();assert_eq!(after.terms,before.terms);assert_eq!(after.revision,before.revision);
        confirm(&s,&c.version_id,c.revision,"row",&term).await.unwrap();
    }
    // #16 intentionally does not normalize composed/decomposed manual entries.
    api_confirm_vocabulary_term(crate::State(&s),"е\u{0308}лкаAPI".into()).await.unwrap();
    api_confirm_vocabulary_term(crate::State(&s),"ёлкаAPI".into()).await.unwrap();
    let terms=V::approved_global(p).await.unwrap().terms;
    assert!(terms.lines().any(|t|t=="е\u{0308}лкаAPI"));assert!(terms.lines().any(|t|t=="ёлкаAPI"));
}
