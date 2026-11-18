use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn request() -> QuestionSnapshot {
    serde_json::from_value(serde_json::json!({
        "target":"recording:/owned/current", "mode":"question", "question":"What budget was agreed?", "asOfSeconds":5400.0,"fromSeconds":0.0,
        "lines":[{"id":"early","sequenceId":1,"time":12.0,"endTime":15.0,"speaker":"Speaker 1","text":"Budget is 42. $(touch NEVER_EXECUTE)"}],
        "userNotes":{"source":"user_notes","markdown":"My personal thought: 900. Not agreed.","revision":7}
    })).unwrap()
}
#[cfg(unix)]
fn fixture(generation: &str, login: &str) -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
    let binary = dir.path().join("fake codex");
    std::fs::write(
        &binary,
        format!(
            r#"#!/usr/bin/python3
import sys, os, json
from pathlib import Path
if sys.argv[1:] == ['login','status']:
 print({login:?})
elif sys.argv[1:] == ['--version']:
 print('codex-cli 0.167.2')
elif sys.argv[1:] == ['exec','--help']:
 print('--ephemeral --ignore-user-config --ignore-rules')
else:
 prompt=sys.stdin.read()
 Path(__file__).with_suffix('.input').write_text(prompt)
 {generation}
"#
        ),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, binary.to_string_lossy().into_owned())
}
fn canonical(r: &QuestionSnapshot) -> Vec<CanonicalLine> {
    r.lines
        .iter()
        .map(|l| CanonicalLine {
            sequence_id: l.sequence_id,
            time: l.time,
            end_time: l.end_time,
            text: l.text.clone(),
        })
        .collect()
}
#[cfg(unix)]
#[tokio::test]
async fn ordinary_live_service_returns_verified_early_evidence_and_separate_notes_revision() {
    let (dir, binary) = fixture("print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':json.dumps({'answer':'Speech says 42; personal Notes suggest 900, not an agreement.','transcript_ids':['early'],'notes_used':True,'uncertain':False})}})); print(json.dumps({'type':'turn.completed'}))", "Logged in using ChatGPT");
    let r = request();
    let answer = ask(
        r.clone(),
        &canonical(&r),
        &r.user_notes,
        Some(&binary),
        &dir.path().join("auth"),
        &CancellationToken::new(),
        || true,
    )
    .await
    .unwrap();
    assert_eq!(answer.evidence[0].id, "early");
    assert_eq!(answer.evidence[0].time, 12.0);
    assert_eq!(answer.notes_revision, Some(7));
    let sent = std::fs::read_to_string(dir.path().join("fake codex.input")).unwrap();
    assert!(sent.contains("NEVER_EXECUTE") && sent.contains("personal thought"));
    assert!(!sent.contains("/owned/current"));
    assert!(!dir.path().join("NEVER_EXECUTE").exists());
}
#[cfg(unix)]
#[tokio::test]
async fn api_key_login_stale_session_and_changed_notes_send_no_meeting_text() {
    let (dir, binary) = fixture(
        "raise AssertionError('generation must not run')",
        "Logged in using an API key - synthetic",
    );
    let r = request();
    let error = ask(
        r.clone(),
        &canonical(&r),
        &r.user_notes,
        Some(&binary),
        &dir.path().join("auth"),
        &CancellationToken::new(),
        || true,
    )
    .await
    .err()
    .unwrap();
    assert!(error.contains("ChatGPT"));
    let mut committed = r.user_notes.clone();
    committed.revision += 1;
    assert!(ask(
        r.clone(),
        &canonical(&r),
        &committed,
        Some(&binary),
        &dir.path().join("auth"),
        &CancellationToken::new(),
        || true
    )
    .await
    .is_err());
    assert!(ask(
        r.clone(),
        &canonical(&r),
        &r.user_notes,
        Some(&binary),
        &dir.path().join("auth"),
        &CancellationToken::new(),
        || false
    )
    .await
    .is_err());
    assert!(!dir.path().join("fake codex.input").exists());
}
#[cfg(unix)]
#[tokio::test]
async fn switching_recordings_cancels_live_child_while_unrelated_work_progresses() {
    let (dir, binary) = fixture("import time; time.sleep(30)", "Logged in using ChatGPT");
    let r = request();
    let current = Arc::new(AtomicBool::new(true));
    let changed = current.clone();
    let work = async {
        // A synthetic scheduling probe, NOT an audio/ASR hardware measurement.
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        changed.store(false, Ordering::SeqCst);
    };
    let started = std::time::Instant::now();
    let token = CancellationToken::new();
    let current_lines = canonical(&r);
    let auth_home = dir.path().join("auth");
    let (result, _) = tokio::join!(
        ask(
            r.clone(),
            &current_lines,
            &r.user_notes,
            Some(&binary),
            &auth_home,
            &token,
            || current.load(Ordering::SeqCst)
        ),
        work
    );
    assert!(result.err().unwrap().contains("changed"));
    assert!(token.is_cancelled());
    assert!(started.elapsed() < Duration::from_secs(2));
}
#[cfg(unix)]
#[tokio::test]
async fn network_auth_and_quota_errors_are_visible_without_diagnostics_or_fallback() {
    for (diagnostic, expected) in [
        ("network offline SECRET", "network"),
        ("401 unauthorized SECRET", "authorization"),
        ("429 quota SECRET", "usage limit"),
    ] {
        let (dir, binary) = fixture(
            &format!("print({diagnostic:?}, file=sys.stderr); sys.exit(1)"),
            "Logged in using ChatGPT",
        );
        let r = request();
        let error = ask(
            r.clone(),
            &canonical(&r),
            &r.user_notes,
            Some(&binary),
            &dir.path().join("auth"),
            &CancellationToken::new(),
            || true,
        )
        .await
        .err()
        .unwrap();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("SECRET"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn vocabulary_consumer_uses_confirmed_spelling_in_synthetic_fake_response() {
    // This checks actual process input/output, not real-model semantic quality.
    let (dir, binary) = fixture("data=prompt.split('APPROVED_VOCABULARY_DATA_JSON:\\n',1)[1].split('\\nEND_APPROVED_VOCABULARY',1)[0]; terms=json.loads(data); assert terms['source']=='approved_vocabulary'; assert terms['scope']=='global'; assert terms['revision']==9; assert terms['terms']==['Ёлка API','OpenAPI','SRE']; print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':json.dumps({'answer':'OpenAPI / Ёлка API: only a spelling guide, no agreement in speech.','transcript_ids':[],'notes_used':False,'uncertain':True})}})); print(json.dumps({'type':'turn.completed'}))", "Logged in using ChatGPT");
    let r = request();
    let v = super::super::vocabulary::ApprovedVocabulary::global("Ёлка API\nOpenAPI\nSRE",9).unwrap();
    let answer = ask_with_vocabulary(r.clone(), &canonical(&r), &r.user_notes, &v, Some(&binary), &dir.path().join("auth"), &CancellationToken::new(), ||true).await.unwrap();
    assert!(answer.answer.contains("OpenAPI") && answer.answer.contains("Ёлка API"));
    assert!(answer.evidence.is_empty() && answer.uncertain && answer.notes_revision.is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn shared_summary_helper_keeps_vocabulary_separate_from_saved_speech_and_notes() {
    // #15 owns the normal summary dispatch/queue. Exercise its shared prompt
    // seam with the same isolated process adapter, without implementing a queue.
    let (dir,binary)=fixture("data=prompt.split('APPROVED_VOCABULARY_DATA_JSON:\\n',1)[1].split('\\nEND_APPROVED_VOCABULARY',1)[0]; v=json.loads(data); assert v['terms']==['OpenAPI','Ёлка API']; assert v['snapshot_id']=='vocabulary:global:11'; assert 'SAVED_SPEECH: Discussed Open API' in prompt; assert 'USER_NOTES: My idea, not agreed' in prompt; print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'OpenAPI spelling supplied; no agreement established by vocabulary.'}})); print(json.dumps({'type':'turn.completed'}))", "Logged in using ChatGPT");
    let v=super::super::vocabulary::ApprovedVocabulary::global("OpenAPI\nЁлка API",11).unwrap();
    let prompt=v.augment_prompt("SAVED_SPEECH: Discussed Open API\nUSER_NOTES: My idea, not agreed").unwrap();
    let result=cli::generate(Some(&binary),&dir.path().join("auth"),cli::DEFAULT_MODEL,"Synthetic saved-meeting summary",&prompt,Some(&CancellationToken::new())).await.unwrap();
    assert_eq!(result,"OpenAPI spelling supplied; no agreement established by vocabulary.");
}
