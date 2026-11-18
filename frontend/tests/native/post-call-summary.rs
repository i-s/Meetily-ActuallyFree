use crate::api::TranscriptSegment;
use crate::store::Revision;
use crate::summary::{Snapshot, Summary};
fn snapshot() -> Snapshot {
    Snapshot {
        revision: Revision {
            meeting_id: "a".into(),
            version_id: "live-a".into(),
            transcript_revision: 3,
            notes_revision: 7,
        },
        lines: vec![TranscriptSegment {
            id: "row-a".into(),
            text: "Alex: I will send the API draft on Friday.".into(),
            timestamp: "2026".into(),
            audio_start_time: Some(1.),
            audio_end_time: Some(2.),
            duration: Some(1.),
            speaker: Some("Speaker 1".into()),
        }],
        notes: "Maybe ask Sam to own rollout".into(),
    }
}
fn response() -> serde_json::Value {
    serde_json::json!({"summary":[{"text":"API draft discussed","evidence":[{"source":"transcript","row_id":"row-a","quote":"API draft"}],"owner":null,"deadline":null}],"decisions":[],"tasks":[{"text":"Send the API draft","evidence":[{"source":"transcript","row_id":"row-a","quote":"Alex: I will send the API draft on Friday."}],"owner":{"value":"Alex","quote":"Alex: I will send"},"deadline":{"value":"Friday","quote":"draft on Friday"}}],"open_questions":[{"text":"Personal note: ask Sam about rollout, not an agreement","evidence":[{"source":"user_notes","row_id":null,"quote":"Maybe ask Sam to own rollout"}],"owner":null,"deadline":null}],"uncertainty":["No confirmed rollout owner"]})
}
#[test]
fn summary_requires_four_sections_and_exact_scoped_evidence() {
    let s = snapshot();
    let valid = response();
    let parsed = Summary::parse(&valid.to_string(), &s).unwrap();
    assert_eq!(parsed.tasks.len(), 1);
    for bad in [
        serde_json::json!({"summary":[]}),
        {
            let mut v = valid.clone();
            v["tasks"][0]["evidence"][0]["row_id"] = "other-meeting".into();
            v
        },
        {
            let mut v = valid.clone();
            v["summary"][0]["evidence"][0]["quote"] = "Invented approval".into();
            v
        },
        {
            let mut v = valid.clone();
            v["tasks"][0]["deadline"]["value"] = "Monday".into();
            v
        },
    ] {
        assert!(
            Summary::parse(&bad.to_string(), &s).is_err(),
            "accepted {bad}"
        );
    }
}
#[test]
fn notes_cannot_be_decisions_tasks_or_attributed_to_participants() {
    let s = snapshot();
    let mut bad = response();
    bad["tasks"][0] = bad["open_questions"][0].clone();
    assert!(Summary::parse(&bad.to_string(), &s).is_err());
    let mut bad = response();
    bad["open_questions"][0]["owner"] = serde_json::json!({"value":"Sam","quote":"ask Sam"});
    assert!(Summary::parse(&bad.to_string(), &s).is_err());
    let prompt = s.prompt().unwrap();
    assert!(prompt.contains("user_notes"));
    assert!(prompt.contains("live-a"));
    assert!(prompt.contains("not participant speech"));
}
#[test]
fn long_context_fails_explicitly_instead_of_silently_truncating() {
    let mut s = snapshot();
    s.notes = "x".repeat(600_000);
    assert!(s.prompt().is_err());
}

#[test]
fn synthetic_ninety_minute_ru_en_text_retains_first_and_last_turn_without_truncation() {
    let mut s=snapshot();
    s.lines=(0..1200).map(|i|TranscriptSegment {
        id:format!("transcript-synthetic-{i:036}"),
        text:format!("Turn {i}: Обсуждаем API, проверяем контракт и сроки. This is synthetic local speech, not real user data. Уточнение решения требует основания в речи."),
        timestamp:"2026-11-18T10:00:00.000Z".into(),audio_start_time:Some(i as f64*4.5),audio_end_time:Some((i+1) as f64*4.5),duration:Some(4.5),speaker:Some("Speaker 1".into()),
    }).collect();
    let prompt=s.prompt().expect("ordinary long RU/EN fixture should fit the bounded full-context path");
    assert!(prompt.contains("Turn 0:"));assert!(prompt.contains("Turn 1199:"));
    assert!(prompt.len()<512*1024);
}
