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
