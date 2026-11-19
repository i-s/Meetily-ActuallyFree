use super::*;
fn request() -> QuestionSnapshot {
    serde_json::from_value(serde_json::json!({
        "target":"recording:/synthetic/current", "mode":"question", "question":"What budget?", "asOfSeconds":5400.0,"fromSeconds":0.0,
        "lines":[{"id":"ui-early","sequenceId":1,"time":12.0,"endTime":15.0,"speaker":"You","text":"Budget is 42"}],
        "userNotes":{"source":"user_notes","markdown":"My idea, not agreed","revision":7}
    })).unwrap()
}
#[test]
fn prompt_separates_personal_thoughts_and_contains_no_meeting_folder_or_other_sources() {
    let r = request();
    let p = r.prompt().unwrap();
    assert!(p.contains("user_notes") && p.contains("not evidence of an agreement"));
    assert!(p.contains("Budget is 42") && p.contains("My idea, not agreed"));
    assert!(!p.contains("/synthetic/current"));
    let answer = r
        .answer(
            r#"{"answer":"42","transcript_ids":["ui-early"],"notes_used":false,"uncertain":false}"#,
        )
        .unwrap();
    assert_eq!(answer.evidence[0].time, 12.0);
    assert_eq!(answer.evidence[0].id, "ui-early");
    assert_eq!(answer.notes_revision, None);
}
#[test]
fn rejects_unknown_evidence_and_unsubstantiated_confident_answers() {
    let r = request();
    for answer in [
        r#"{"answer":"42","transcript_ids":["another-meeting"],"notes_used":false,"uncertain":false}"#,
        r#"{"answer":"Agreed","transcript_ids":[],"notes_used":false,"uncertain":false}"#,
        r#"{"answer":"Agreed","transcript_ids":[],"notes_used":true,"uncertain":false}"#,
        "partial response",
    ] {
        assert!(r.answer(answer).is_err());
    }
    let personal = r.answer(r#"{"answer":"Only a personal idea; no agreement established","transcript_ids":[],"notes_used":true,"uncertain":true}"#).unwrap();
    assert_eq!(personal.notes_revision, Some(7));
    assert!(personal.evidence.is_empty());
}
#[test]
fn native_context_rejects_wrong_window_partial_future_invalid_and_oversize_turns() {
    let mut r = request();
    r.mode = QuestionMode::Missed;
    r.from_seconds = 5100.0;
    assert!(
        r.prompt().is_err(),
        "old evidence must not bypass the window"
    );
    r.lines[0].time = 5100.0;
    r.lines[0].end_time = 5101.0;
    assert!(r.prompt().is_ok());
    r.from_seconds = 5000.0;
    assert!(r.prompt().is_err());
    r.from_seconds = 5100.0;
    r.lines[0].end_time = 5401.0;
    assert!(r.prompt().is_err());
    r.lines[0].end_time = f64::NAN;
    assert!(r.prompt().is_err());
    r.lines[0].end_time = 5101.0;
    r.lines[0].text = "a".repeat(600_000);
    assert!(r.prompt().is_err());
}
#[test]
fn current_native_transcript_validation_refuses_stale_text_and_another_session_sequence() {
    let r = request();
    let current = vec![CanonicalLine {
        sequence_id: 1,
        time: 12.0,
        end_time: 15.0,
        text: "Budget is 42".into(),
    }];
    assert!(r.validate_current(&current).is_ok());
    assert!(r.validate_current(&[]).is_err());
    let mut edited = current.clone();
    edited[0].text = "other meeting".into();
    assert!(r.validate_current(&edited).is_err());
    edited = current;
    edited[0].end_time = 14.0;
    assert!(r.validate_current(&edited).is_err());
}

#[test]
fn approved_spelling_is_bounded_separate_context_never_speech_evidence() {
    let r = request();
    let vocabulary = super::super::vocabulary::ApprovedVocabulary::global("Ёлка API\nOpenAPI\nSRE", 4).unwrap();
    let p = vocabulary.augment_prompt(&r.prompt().unwrap()).unwrap();
    assert!(p.contains("approved_vocabulary") && p.contains("global") && p.contains("Ёлка API"));
    assert!(p.contains("not evidence") && p.contains("spelling"));
    assert!(!p.contains("/synthetic/current"));
    for raw in [
        r#"{"answer":"SRE was agreed","transcript_ids":["approved_vocabulary"],"notes_used":false,"uncertain":false}"#,
        r#"{"answer":"SRE was agreed","transcript_ids":[],"notes_used":false,"uncertain":false}"#,
    ] { assert!(r.answer(raw).is_err()); }
    let unknown = r.answer(r#"{"answer":"No speech establishes SRE or an agreement","transcript_ids":[],"notes_used":false,"uncertain":true}"#).unwrap();
    assert!(unknown.evidence.is_empty());
    assert!(super::super::vocabulary::ApprovedVocabulary::global(&"я".repeat(1001), 4).is_err());
    assert!(super::super::vocabulary::ApprovedVocabulary::global("term", -1).is_err());
    assert!(vocabulary.augment_prompt(&"x".repeat(512 * 1024)).is_err());
}

#[test]
fn frontend_cannot_smuggle_unconfirmed_vocabulary_into_live_request() {
    let mut json=serde_json::to_value(request()).unwrap();
    json["approvedVocabulary"]=serde_json::json!({"terms":["UnconfirmedSuggestion"]});
    assert!(serde_json::from_value::<QuestionSnapshot>(json).is_err());
}
