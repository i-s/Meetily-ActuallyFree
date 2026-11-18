//! Actual ordinary/candidate command bodies and version SQL; synthetic inference.
use crate::{command, database::transcript_versions as versions, state::{AppState, DbManager}, State};
use std::{collections::HashMap, sync::atomic::Ordering};
use serde_json::{json, Value};

struct Fixture { folder: tempfile::TempDir, external: tempfile::TempDir, state: AppState }
impl Fixture {
    async fn new(metadata: Value, tracks: bool) -> Self {
        let folder = tempfile::tempdir().unwrap(); let external = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("metadata.json"), serde_json::to_vec(&metadata).unwrap()).unwrap();
        std::fs::write(folder.path().join("audio.mp4"), b"synthetic decode boundary").unwrap();
        std::fs::write(external.path().join("temporary.wav"), b"external temporary WAV boundary").unwrap();
        if tracks { for name in ["mic.mp4", "system.mp4"] { std::fs::write(folder.path().join(name), b"synthetic source").unwrap(); } }
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        crate::test_migrate(&pool).await;
        sqlx::query("INSERT INTO meetings(id,title,folder_path,created_at,updated_at) VALUES('m','Synthetic meeting',?,'2026','2026')").bind(folder.path().to_str().unwrap()).execute(&pool).await.unwrap();
        for (id, label, start, end) in [("mic","Microphone",0.,2.),("remote","Guest",0.,2.),("manual","Alice + You",2.,4.),("gap","You + Guest",5.,6.)] {
            sqlx::query("INSERT INTO transcripts(id,meeting_id,transcript,timestamp,audio_start_time,audio_end_time,duration,speaker) VALUES(?,'m',?,'2026',?,?,?,?)").bind(id).bind(format!("Exact RU/EN {id}: исправлено!")).bind(start).bind(end).bind(end-start).bind(label).execute(&pool).await.unwrap();
        }
        // Commit baseline initialization through the public selected interface.
        versions::selected(&pool,"m").await.unwrap();
        command::voiceprint::READS.store(0,Ordering::SeqCst); command::voiceprint::WRITES.store(0,Ordering::SeqCst);
        command::DECODES.store(0,Ordering::SeqCst); command::FAIL_ENGINE.store(false,Ordering::SeqCst);
        Self {folder,external,state:AppState{db_manager:DbManager(pool)}}
    }
    async fn ordinary(&self) -> Result<command::MeetingDiarizationResult,String> {
        command::diarize_meeting(State(&self.state),"m".into(),Some(self.external.path().join("temporary.wav").to_string_lossy().into_owned()),None,None,Some("pyannote".into())).await
    }
    async fn candidate(&self, metadata: Value, engine: &str) -> String {
        let id = versions::begin(self.state.db_manager.pool(),"m",&json!({"recording_metadata":metadata,"label_origin":"generated_asr","diarization_engine":engine,"diarization_config":{"engine":engine,"nemotron_max_speakers":8,"nemotron_threshold":0.5,"pyannote_threshold":0.6}})).await.unwrap();
        let segments = [row("mic","Microphone",0.,2.),row("remote","Guest",0.,2.),row("gap","You + Guest",5.,6.),row("named_gap","Alice+You",5.,6.)];
        versions::finish_with_sources(self.state.db_manager.pool(),&id,&segments,&HashMap::from([("mic".into(),"microphone".into()),("remote".into(),"system".into()),("gap".into(),"system".into()),("named_gap".into(),"system".into())])).await.unwrap(); id
    }
}
fn row(id:&str,label:&str,start:f64,end:f64)->crate::api::TranscriptSegment {
    crate::api::TranscriptSegment{id:id.into(),text:format!("Candidate RU/EN {id}"),speaker:Some(label.into()),timestamp:"2026-11-18T10:00:01Z".into(),audio_start_time:Some(start),audio_end_time:Some(end),duration:Some(end-start)}
}
async fn projection(f:&Fixture)->String { serde_json::to_string(&versions::selected(f.state.db_manager.pool(),"m").await.unwrap()).unwrap() }

#[tokio::test]
async fn original_flagged_policy_precedes_external_temporary_wav_and_polluted_voiceprint() {
    let f=Fixture::new(json!({"microphone_identity_unverified":true}),false).await;
    let before=versions::selected(f.state.db_manager.pool(),"m").await.unwrap();
    let polluted=b"synthetic polluted legacy enrollment bytes";
    let enrollment=f.folder.path().join("legacy-voiceprint.json");std::fs::write(&enrollment,polluted).unwrap();
    f.ordinary().await.unwrap();
    assert_eq!(command::voiceprint::READS.load(Ordering::SeqCst),0,"original policy, not external WAV parent, owns identity");
    assert_eq!(command::voiceprint::WRITES.load(Ordering::SeqCst),0);
    let after=versions::selected(f.state.db_manager.pool(),"m").await.unwrap();
    for original in &before.segments {
        let current=after.segments.iter().find(|s|s.id==original.id).unwrap();
        assert_eq!((&current.text,&current.timestamp,current.audio_start_time,current.audio_end_time),(&original.text,&original.timestamp,original.audio_start_time,original.audio_end_time));
        assert!(!current.speaker.as_deref().unwrap().split('+').any(|part|part.trim()=="You"));
    }
    assert_eq!(after.segments.iter().find(|s|s.id=="manual").unwrap().speaker.as_deref(),Some("Alice"));
    assert_eq!(after.segments.iter().find(|s|s.id=="gap").unwrap().speaker.as_deref(),Some("Guest"));
    let historical: String=sqlx::query_scalar("SELECT segments_json FROM transcript_versions WHERE kind='live'").fetch_one(f.state.db_manager.pool()).await.unwrap();
    assert!(historical.contains("Alice + You"));
    assert_eq!(std::fs::read(enrollment).unwrap(),polluted);
}

#[tokio::test]
async fn flagged_candidate_keeps_overlapping_sources_through_selection_correction_restore() {
    for engine in ["pyannote","nemotron"] {
        let metadata=json!({"microphone_identity_unverified":true,"opaque_policy":{"future":"retain"}});
        let f=Fixture::new(metadata.clone(),true).await;
        let pool=f.state.db_manager.pool();
        let before=projection(&f).await;
        let id=f.candidate(metadata.clone(),engine).await;
        command::diarize_version(&f.state,"m",&id).await.unwrap();
        assert_eq!(projection(&f).await,before,"candidate cannot replace selected manual text/labels");
        assert_eq!(command::voiceprint::READS.load(Ordering::SeqCst),0);assert_eq!(command::voiceprint::WRITES.load(Ordering::SeqCst),0);
        let candidate=versions::segments(pool,"m",&id).await.unwrap();
        assert_eq!(candidate.iter().find(|s|s.id=="mic").unwrap().speaker.as_deref(),Some("Microphone"));
        assert_eq!(candidate.iter().find(|s|s.id=="named_gap").unwrap().speaker.as_deref(),Some("Alice"));
        assert_eq!(candidate.iter().find(|s|s.id=="gap").unwrap().speaker.as_deref(),Some("Guest"));
        for s in &candidate {assert!(!s.speaker.as_deref().unwrap().contains("You"));}
        assert_eq!(candidate.iter().find(|s|s.id=="mic").unwrap().audio_start_time,candidate.iter().find(|s|s.id=="remote").unwrap().audio_start_time);
        versions::set_status(pool,&id,"ready",None).await.unwrap();
        let selected=versions::selected(pool,"m").await.unwrap();versions::select(pool,"m",&id,selected.revision).await.unwrap();
        let selected=versions::selected(pool,"m").await.unwrap();
        versions::correct_text(pool,"m","mic","Manual correction RU/EN",selected.revision).await.unwrap();
        versions::checkpoint_current(pool,"m").await.unwrap();
        let correction:String=sqlx::query_scalar("SELECT id FROM transcript_versions WHERE kind='corrections' AND segments_json LIKE '%Manual correction RU/EN%' ORDER BY created_at DESC LIMIT 1").fetch_one(pool).await.unwrap();
        let selected=versions::selected(pool,"m").await.unwrap();versions::select(pool,"m",&id,selected.revision).await.unwrap();
        let selected=versions::selected(pool,"m").await.unwrap();versions::select(pool,"m",&correction,selected.revision).await.unwrap();
        let restored=versions::selected(pool,"m").await.unwrap();assert_eq!(restored.segments.iter().find(|s|s.id=="mic").unwrap().text,"Manual correction RU/EN");
        let settings:Value=serde_json::from_str(&restored.settings_json).unwrap();assert_eq!(settings["recording_metadata"],metadata);
        assert_eq!(settings["source_map"]["mic"],"microphone");assert_eq!(settings["source_map"]["remote"],"system");
        for s in &restored.segments {assert!(!s.speaker.as_deref().unwrap().contains("You"));}
    }
}

#[tokio::test]
async fn stored_policy_cannot_be_weakened_by_current_file_for_either_entrypoint() {
    let f=Fixture::new(json!({}),true).await;
    let pool=f.state.db_manager.pool();
    sqlx::query("UPDATE transcript_versions SET settings_json=? WHERE id=(SELECT version_id FROM transcript_selection WHERE meeting_id='m')").bind(json!({"recording_metadata":{"microphone_identity_unverified":true,"opaque":7}}).to_string()).execute(pool).await.unwrap();
    f.ordinary().await.unwrap();assert_eq!(command::voiceprint::READS.load(Ordering::SeqCst),0);
    let id=f.candidate(json!({}),"pyannote").await;command::diarize_version(&f.state,"m",&id).await.unwrap();
    let segments=versions::segments(pool,"m",&id).await.unwrap();assert_eq!(segments[0].speaker.as_deref(),Some("Microphone"));
    assert_eq!(command::voiceprint::READS.load(Ordering::SeqCst),0);
}

#[tokio::test]
async fn malformed_file_or_stored_metadata_aborts_before_decode_and_retains_projection() {
    for raw in ["{","null","[]","true","1","\"scalar\"","{\"microphone_identity_unverified\":\"bad\"}"] {
        let f=Fixture::new(json!({}),true).await;let id=f.candidate(json!({}),"pyannote").await;let before=projection(&f).await;
        std::fs::write(f.folder.path().join("metadata.json"),raw).unwrap();
        assert!(f.ordinary().await.is_err(),"ordinary accepted {raw}");assert!(command::diarize_version(&f.state,"m",&id).await.is_err(),"candidate accepted {raw}");
        assert_eq!(command::DECODES.load(Ordering::SeqCst),0,"decoded before metadata validation: {raw}");assert_eq!(projection(&f).await,before);
    }
    for malformed in [Value::Null,json!([]),json!(false),json!(42),json!("scalar"),json!({"in_person":"bad"})] {
        let f=Fixture::new(json!({}),true).await;let id=f.candidate(malformed.clone(),"pyannote").await;let before=projection(&f).await;
        assert!(command::diarize_version(&f.state,"m",&id).await.is_err(),"stored candidate metadata accepted {malformed}");
        assert_eq!(command::DECODES.load(Ordering::SeqCst),0);assert_eq!(projection(&f).await,before);
    }
}

#[tokio::test]
async fn selected_engine_failure_preserves_previous_projection_and_candidate_source_map() {
    let f=Fixture::new(json!({}),true).await;let id=f.candidate(json!({}),"pyannote").await;let before=projection(&f).await;
    let source_before=versions::version_sources(f.state.db_manager.pool(),"m",&id).await.unwrap();
    command::FAIL_ENGINE.store(true,Ordering::SeqCst);
    let failed=command::diarize_version(&f.state,"m",&id).await;
    command::FAIL_ENGINE.store(false,Ordering::SeqCst);
    assert!(failed.is_err());assert_eq!(projection(&f).await,before);
    assert_eq!(versions::version_sources(f.state.db_manager.pool(),"m",&id).await.unwrap(),source_before);
}

#[tokio::test]
async fn stored_nonobject_metadata_also_blocks_ordinary_external_wav_before_decode() {
    for metadata in [Value::Null,json!([]),json!(true),json!(7),json!("scalar"),json!({"microphone_identity_unverified":null})] {
        let f=Fixture::new(json!({}),false).await;
        sqlx::query("UPDATE transcript_versions SET settings_json=? WHERE id=(SELECT version_id FROM transcript_selection WHERE meeting_id='m')").bind(json!({"recording_metadata":metadata}).to_string()).execute(f.state.db_manager.pool()).await.unwrap();
        let before=projection(&f).await;
        assert!(f.ordinary().await.is_err(),"ordinary accepted stored {metadata}");
        assert_eq!(command::DECODES.load(Ordering::SeqCst),0);assert_eq!(projection(&f).await,before);
    }
}

#[tokio::test]
async fn stricter_current_policy_is_frozen_with_candidate_not_lost_on_later_selection() {
    let f=Fixture::new(json!({}),true).await;let pool=f.state.db_manager.pool();
    let id=f.candidate(json!({"opaque":"old object"}),"pyannote").await;
    std::fs::write(f.folder.path().join("metadata.json"),br#"{"microphone_identity_unverified":true}"#).unwrap();
    command::diarize_version(&f.state,"m",&id).await.unwrap();versions::set_status(pool,&id,"ready",None).await.unwrap();
    let selected=versions::selected(pool,"m").await.unwrap();versions::select(pool,"m",&id,selected.revision).await.unwrap();
    std::fs::write(f.folder.path().join("metadata.json"),b"{}").unwrap();
    command::voiceprint::READS.store(0,Ordering::SeqCst);
    f.ordinary().await.unwrap();assert_eq!(command::voiceprint::READS.load(Ordering::SeqCst),0);
    let selected=versions::selected(pool,"m").await.unwrap();let settings:Value=serde_json::from_str(&selected.settings_json).unwrap();assert_eq!(settings["recording_metadata"]["opaque"],"old object");
    for s in selected.segments {assert!(!s.speaker.as_deref().unwrap().contains("You"));}
}

#[tokio::test]
async fn in_person_policy_never_runs_room_identity_or_restores_generated_you() {
    let f=Fixture::new(json!({"in_person":true}),true).await;
    let id=f.candidate(json!({"in_person":true}),"pyannote").await;let before=projection(&f).await;
    assert!(f.ordinary().await.is_err());command::diarize_version(&f.state,"m",&id).await.unwrap();
    assert_eq!(command::DECODES.load(Ordering::SeqCst),0);assert_eq!(command::voiceprint::READS.load(Ordering::SeqCst),0);
    assert_eq!(projection(&f).await,before);
    for s in versions::segments(f.state.db_manager.pool(),"m",&id).await.unwrap() {assert!(!s.speaker.as_deref().unwrap().contains("You"));}
}

#[tokio::test]
async fn restoring_older_weak_snapshot_cannot_regrant_session_identity() {
    let f=Fixture::new(json!({}),true).await;let pool=f.state.db_manager.pool();
    let old=versions::selected(pool,"m").await.unwrap();
    let id=f.candidate(json!({"microphone_identity_unverified":true}),"pyannote").await;
    command::diarize_version(&f.state,"m",&id).await.unwrap();versions::set_status(pool,&id,"ready",None).await.unwrap();
    versions::select(pool,"m",&id,old.revision).await.unwrap();let current=versions::selected(pool,"m").await.unwrap();
    versions::select(pool,"m",&old.version_id,current.revision).await.unwrap();
    command::voiceprint::READS.store(0,Ordering::SeqCst);f.ordinary().await.unwrap();
    assert_eq!(command::voiceprint::READS.load(Ordering::SeqCst),0);
    for s in versions::selected(pool,"m").await.unwrap().segments {assert!(!s.speaker.as_deref().unwrap().contains("You"));}
}

#[tokio::test]
async fn actual_asr_producer_preflight_respects_frozen_policy_before_source_decode() {
    let f=Fixture::new(json!({}),true).await;
    let id=f.candidate(json!({"microphone_identity_unverified":true}),"pyannote").await;
    let sources=crate::audio::retranscription::asr_preflight(&f.state,"m".into(),f.folder.path().to_string_lossy().into_owned(),id.clone()).await.unwrap();
    assert_eq!(sources,vec![("microphone".into(),Some("Microphone".into())),("system".into(),Some("Guest".into()))]);
    for raw in ["{","null","[]","true","1","\"scalar\""] {
        std::fs::write(f.folder.path().join("metadata.json"),raw).unwrap();
        assert!(crate::audio::retranscription::asr_preflight(&f.state,"m".into(),f.folder.path().to_string_lossy().into_owned(),id.clone()).await.is_err());
    }
    assert_eq!(command::DECODES.load(Ordering::SeqCst),0);
}

#[tokio::test]
async fn candidate_identity_labels_and_authority_roll_back_together_on_sql_failure() {
    let f=Fixture::new(json!({"microphone_identity_unverified":true}),true).await;let pool=f.state.db_manager.pool();
    let id=f.candidate(json!({}),"pyannote").await;let selected=projection(&f).await;
    let before:(String,String)=sqlx::query_as("SELECT segments_json,settings_json FROM transcript_versions WHERE id=?").bind(&id).fetch_one(pool).await.unwrap();
    sqlx::raw_sql("CREATE TRIGGER fail_identity_commit BEFORE UPDATE OF segments_json ON transcript_versions BEGIN SELECT RAISE(ABORT,'injected identity commit failure'); END;").execute(pool).await.unwrap();
    assert!(command::diarize_version(&f.state,"m",&id).await.is_err());
    let after:(String,String)=sqlx::query_as("SELECT segments_json,settings_json FROM transcript_versions WHERE id=?").bind(&id).fetch_one(pool).await.unwrap();
    assert_eq!(after,before);assert_eq!(projection(&f).await,selected);
}
