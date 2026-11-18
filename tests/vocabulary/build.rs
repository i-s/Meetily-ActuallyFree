use std::{env, fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let worker = root.join("frontend/src-tauri/src/audio/transcription/worker.rs");
    let engine = root.join("frontend/src-tauri/src/whisper_engine/whisper_engine.rs");
    println!("cargo:rerun-if-changed={}", worker.display());
    println!("cargo:rerun-if-changed={}", engine.display());
    let worker=fs::read_to_string(worker).unwrap();
    let dispatch=worker.split("async fn transcribe_chunk_with_provider(").nth(1).unwrap().split("/// Format current timestamp").next().unwrap();
    let engine=fs::read_to_string(engine).unwrap();
    let limit=engine.lines().find(|l|l.starts_with("const MAX_INITIAL_PROMPT_TOKENS:")).unwrap();
    let tokenize=engine.split("    fn tokenize_initial_prompt(").nth(1).unwrap().split("    /// Detect available GPU acceleration capabilities").next().unwrap();
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("asr.rs"),format!("{limit}\nimpl WhisperEngine {{fn tokenize_initial_prompt({tokenize}}}\nasync fn transcribe_chunk_with_provider({dispatch}")).unwrap();
    let api_path=root.join("frontend/src-tauri/src/api/api.rs");
    println!("cargo:rerun-if-changed={}",api_path.display());
    let api=fs::read_to_string(api_path).unwrap();
    let dto=api.split("pub struct WhisperVocabularyConfig {").nth(1).unwrap().split('}').next().unwrap();
    let commands=api.split("pub async fn api_get_whisper_vocabulary(").nth(1).unwrap().split("#[tauri::command]\npub async fn api_get_transcript_api_key").next().unwrap();
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("commands.rs"),format!("#[derive(serde::Serialize)] #[serde(rename_all=\"camelCase\")] pub struct WhisperVocabularyConfig {{{dto}}}\npub async fn api_get_whisper_vocabulary({}",commands.replace("#[tauri::command]", ""))).unwrap();

}
