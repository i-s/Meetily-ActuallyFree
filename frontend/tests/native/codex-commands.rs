//! Portable transport tests compile the production command bodies. Only Tauri,
//! recorder ownership and SQLite transport are substituted by the harness.
use crate::{commands::*, context::QuestionSnapshot};
#[tokio::test]
async fn normal_native_command_uses_saved_absolute_path_checks_owner_and_notes_before_text_dispatch() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("selected codex");
    std::fs::write(&binary, r#"#!/usr/bin/python3
import sys, json
from pathlib import Path
if sys.argv[1:] == ['login','status']: print('Logged in using ChatGPT')
elif sys.argv[1:] == ['--version']: print('codex-cli 0.167.2')
elif sys.argv[1:] == ['exec','--help']: print('--ephemeral --ignore-user-config --ignore-rules')
else:
 Path(__file__).with_suffix('.input').write_text(sys.stdin.read())
 print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':json.dumps({'answer':'42 from speech; personal Notes are not an agreement.','transcript_ids':['early'],'notes_used':True,'uncertain':False})}}))
 print(json.dumps({'type':'turn.completed'}))
"#).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    crate::test_runtime::configure(dir.path());
    codex_cli_save_path(Some(binary.to_string_lossy().into())).await.unwrap();
    assert_eq!(codex_cli_get_path().await.unwrap().unwrap(), binary.to_string_lossy());
    let status = codex_cli_get_status(None).await.unwrap();
    assert!(status.installed && status.logged_in);
    let cutoff = codex_live_cutoff().await.unwrap();
    assert_eq!(cutoff.as_of_seconds, 600.0, "pause-excluding active clock, not wall clock 1200");
    assert_eq!(cutoff.target, "recording:/synthetic/current");
    let request: QuestionSnapshot = serde_json::from_value(serde_json::json!({
      "target":"recording:/synthetic/current", "mode":"question", "question":"What budget?", "asOfSeconds":600.0,"fromSeconds":0.0,
      "lines":[{"id":"early","sequenceId":1,"time":12.0,"endTime":15.0,"speaker":"Custom display label","source":"UNTRUSTED_PROVENANCE","text":"Budget 42"}],
      "userNotes":{"source":"user_notes","markdown":"My idea, not agreed","revision":7}
    })).unwrap();
    let state = crate::state::AppState { db_manager: crate::state::DbManager };
    let answer = codex_live_ask(crate::State(&state), "native-request-A".into(), request.clone()).await.unwrap();
    assert_eq!(answer.evidence[0].time, 12.0);
    assert_eq!(serde_json::to_value(&answer).unwrap()["evidence"][0]["sequenceId"], 1);
    assert_eq!(answer.notes_revision, Some(7));
    let prompt = std::fs::read_to_string(binary.with_extension("input")).unwrap();
    assert!(!prompt.contains("UNTRUSTED_PROVENANCE") && prompt.contains("You"));
    assert!(!prompt.contains("/synthetic/current"));
    std::fs::remove_file(binary.with_extension("input")).unwrap();
    let mut stale = request.clone(); stale.target = "recording:/synthetic/other".into();
    assert!(codex_live_ask(crate::State(&state), "native-request-B".into(), stale).await.is_err());
    let mut future = request.clone(); future.as_of_seconds = 900.0;
    assert!(codex_live_ask(crate::State(&state), "native-request-C".into(), future).await.is_err());
    let mut notes = request.clone(); notes.user_notes.revision = 8;
    assert!(codex_live_ask(crate::State(&state), "native-request-D".into(), notes).await.is_err());
    codex_live_cancel("native-early-cancel".into());
    assert!(codex_live_ask(crate::State(&state), "native-early-cancel".into(), request).await.is_err());
    assert!(!binary.with_extension("input").exists());
    assert_eq!(crate::test_runtime::note_targets(), vec!["recording:/synthetic/current", "recording:/synthetic/current"]);
}
