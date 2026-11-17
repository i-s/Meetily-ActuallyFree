//! Serial, bounded transcript persistence. Capture/event delivery never waits for
//! SQLite; AI snapshots explicitly wait for the queued-write barrier instead.
use crate::api::TranscriptSegment;
use crate::database::repositories::transcript::TranscriptsRepository;
use crate::database::repositories::person::{PeopleRepository, SpeakerRenameOutcome};
use std::collections::HashMap;
use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

enum Write {
    Segment(TranscriptSegment),
    Barrier(oneshot::Sender<()>),
    Rename(String, String, oneshot::Sender<Result<SpeakerRenameOutcome, String>>),
    Reassign(String, String, oneshot::Sender<Result<SpeakerRenameOutcome, String>>),
}

#[derive(Clone)]
pub struct RecordingWriter {
    sender: mpsc::Sender<Write>,
    error: Arc<Mutex<Option<String>>>,
    on_error: Arc<dyn Fn(String) + Send + Sync>,
}

impl RecordingWriter {
    pub fn start(pool: SqlitePool, meeting_id: String, on_error: impl Fn(String) + Send + Sync + 'static) -> (Self, JoinHandle<()>) {
        let (sender, mut receiver) = mpsc::channel(256);
        let writer = Self { sender, error: Arc::new(Mutex::new(None)), on_error: Arc::new(on_error) };
        let error = writer.error.clone();
        let on_error = writer.on_error.clone();
        let task = tokio::spawn(async move {
            let mut aliases: HashMap<String, String> = HashMap::new();
            while let Some(message) = receiver.recv().await {
                match message {
                    Write::Segment(mut segment) => {
                        if let Some(label) = segment.speaker.as_ref().and_then(|label| aliases.get(label)) { segment.speaker = Some(label.clone()); }
                        if let Err(cause) = TranscriptsRepository::persist_live_segment(&pool, &meeting_id, &segment).await {
                            report_error(&error, &on_error, format!("Could not save the live transcript: {cause}. Stop retries the retained transcript; AI is unavailable until it is saved."));
                        }
                    }
                    Write::Barrier(reply) => { let _ = reply.send(()); }
                    Write::Rename(from, to, reply) => {
                        let result = PeopleRepository::rename_meeting_speaker(&pool, &meeting_id, &from, &to).await.map_err(|error| error.to_string());
                        if let Ok(outcome) = &result {
                            for label in aliases.values_mut() { if label == &from { *label = outcome.speaker.clone(); } }
                            aliases.insert(from, outcome.speaker.clone());
                        }
                        let _ = reply.send(result);
                    }
                    Write::Reassign(id, to, reply) => {
                        let result = PeopleRepository::reassign_transcript_speaker(&pool, &meeting_id, &id, &to).await.map_err(|error| error.to_string());
                        let _ = reply.send(result);
                    }
                }
            }
        });
        (writer, task)
    }

    pub fn publish(&self, segment: TranscriptSegment) {
        if let Err(cause) = self.sender.try_send(Write::Segment(segment)) {
            report_error(&self.error, &self.on_error, format!("Live transcript storage could not keep up: {cause}. Stop retries the retained transcript; AI is unavailable until it is saved."));
        }
    }

    pub async fn rename(&self, from: String, to: String) -> Result<SpeakerRenameOutcome, String> {
        let (reply, completion) = oneshot::channel();
        self.sender.send(Write::Rename(from, to, reply)).await.map_err(|_| "Transcript storage stopped".to_string())?;
        completion.await.map_err(|_| "Transcript storage stopped".to_string())?
    }

    pub async fn reassign(&self, id: String, to: String) -> Result<SpeakerRenameOutcome, String> {
        let (reply, completion) = oneshot::channel();
        self.sender.send(Write::Reassign(id, to, reply)).await.map_err(|_| "Transcript storage stopped".to_string())?;
        completion.await.map_err(|_| "Transcript storage stopped".to_string())?
    }

    pub async fn flush(&self) -> Result<(), String> {
        let (reply, completion) = oneshot::channel();
        self.sender.send(Write::Barrier(reply)).await.map_err(|_| "Transcript storage stopped".to_string())?;
        completion.await.map_err(|_| "Transcript storage stopped".to_string())?;
        match self.error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn report_error(error: &Mutex<Option<String>>, notify: &Arc<dyn Fn(String) + Send + Sync>, message: String) {
    let first = { let mut error = error.lock().unwrap(); if error.is_none() { *error = Some(message.clone()); true } else { false } };
    if first { notify(message); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn recording_identity_speaker_edits_reach_snapshot_and_future_turns() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::raw_sql("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT, created_at TEXT, updated_at TEXT, folder_path TEXT, title_is_manual INTEGER); CREATE TABLE transcripts (id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, timestamp TEXT, audio_start_time REAL, audio_end_time REAL, duration REAL, speaker TEXT); CREATE TABLE people (id TEXT PRIMARY KEY, display_name TEXT NOT NULL, normalized_name TEXT NOT NULL UNIQUE, notes TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL); CREATE TABLE person_speakers (person_id TEXT NOT NULL, meeting_id TEXT NOT NULL, speaker_label TEXT NOT NULL, UNIQUE(meeting_id,speaker_label));").execute(&pool).await.unwrap();
        let id = TranscriptsRepository::save_transcript(&pool, "Live", &[], None, None).await.unwrap();
        let (writer, task) = RecordingWriter::start(pool.clone(), id.clone(), |_| {});
        let turn = |n| TranscriptSegment { id:format!("{id}:seg_{n}"), text:"Exact words".into(), timestamp:"00:00".into(), audio_start_time:Some(n as f64), audio_end_time:Some(n as f64 + 1.0), duration:Some(1.0), speaker:Some("Speaker 1".into()) };
        writer.publish(turn(1));
        writer.rename("Speaker 1".into(), "Alice".into()).await.unwrap();
        writer.publish(turn(2));
        writer.reassign(format!("{id}:seg_1"), "Bob".into()).await.unwrap();
        writer.flush().await.unwrap();
        let labels: Vec<String> = sqlx::query_scalar("SELECT speaker FROM transcripts ORDER BY audio_start_time").fetch_all(&pool).await.unwrap();
        assert_eq!(labels, ["Bob", "Alice"]);
        drop(writer); task.await.unwrap();
        // Native recovery retains raw provenance, but retrying those raw turns
        // must not undo display attribution already committed to SQLite.
        TranscriptsRepository::finish_recording(&pool, &id, "Stopped", &[turn(1),turn(2)], None).await.unwrap();
        let labels: Vec<String> = sqlx::query_scalar("SELECT speaker FROM transcripts ORDER BY audio_start_time").fetch_all(&pool).await.unwrap();
        assert_eq!(labels, ["Bob", "Alice"]);
    }

    #[tokio::test]
    async fn recording_identity_queue_overflow_is_visible_and_blocks_snapshot() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let observed = errors.clone();
        let (writer, task) = RecordingWriter::start(pool, "recording".into(), move |message| observed.lock().unwrap().push(message));
        // No await: this single-thread Tokio test cannot consume until after
        // the bounded queue has filled, without relying on a timing delay.
        for n in 0..257 {
            writer.publish(TranscriptSegment { id:format!("recording:seg_{n}"), text:"Retained".into(), timestamp:"00:00".into(), audio_start_time:Some(0.0), audio_end_time:Some(1.0), duration:Some(1.0), speaker:None });
        }
        assert_eq!(errors.lock().unwrap().len(), 1);
        assert!(writer.flush().await.unwrap_err().contains("could not keep up"));
        drop(writer); task.await.unwrap();
    }

    #[tokio::test]
    async fn recording_identity_barrier_reports_sql_failure_before_ai_snapshot() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let observed = errors.clone();
        let (writer, task) = RecordingWriter::start(pool, "missing-meeting".into(), move |message| observed.lock().unwrap().push(message));
        writer.publish(TranscriptSegment { id:"missing:seg_1".into(), text:"Retained in native buffer".into(), timestamp:"00:00".into(), audio_start_time:Some(0.0), audio_end_time:Some(1.0), duration:Some(1.0), speaker:None });
        assert!(writer.flush().await.unwrap_err().contains("Could not save"));
        assert_eq!(errors.lock().unwrap().len(), 1);
        drop(writer);
        task.await.unwrap();
    }
}
