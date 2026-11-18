//! Execute unchanged production live-dispatch and token-budget bodies. Model,
//! tokenizer and hardware are fakes: this proves plumbing, NOT ASR accuracy.
use log::{error, info, warn};
use std::sync::Mutex;
use crate::vocabulary::VocabularyRepository as V;
type WhisperToken=i32;
type Result<T> = std::result::Result<T,String>;
struct WhisperContext;
impl WhisperContext { fn tokenize(&self,prompt:&str,_budget:usize)->Result<Vec<WhisperToken>> { Ok(prompt.chars().map(|c|c as i32).collect()) } }
struct WhisperEngine { calls:Mutex<Vec<(Vec<f32>,Vec<i32>)>> }
impl WhisperEngine {
    async fn transcribe_audio_with_confidence(&self, audio:Vec<f32>,_language:Option<String>,prompt:Option<&str>)->Result<(String,f32,bool)> {
        self.calls.lock().unwrap().push((audio,Self::tokenize_initial_prompt(&WhisperContext,prompt)?));
        Ok(("synthetic engine output".into(),1.0,false))
    }
}
struct Parakeet;
impl Parakeet { async fn transcribe_audio(&self,_audio:Vec<f32>)->Result<String>{Ok("parakeet output".into())} }
struct Generic;
struct GenericResult {text:String,confidence:Option<f32>,is_partial:bool}
impl Generic {
 async fn transcribe(&self,_audio:Vec<f32>,_language:Option<String>)->std::result::Result<GenericResult,TranscriptionError>{unreachable!()}
 fn provider_name(&self)->&'static str{"fake"}
}
#[allow(dead_code)] enum TranscriptionEngine {Whisper(WhisperEngine),Parakeet(Parakeet),Provider(Generic)}
#[derive(Debug)] enum TranscriptionError {AudioTooShort {samples:usize,minimum:usize},EngineFailed(String)}
impl std::fmt::Display for TranscriptionError {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{self:?}")}}
struct AudioChunk {sample_rate:u32,data:Vec<f32>,chunk_id:usize}
include!(concat!(env!("OUT_DIR"),"/asr.rs"));
#[tokio::test]
async fn same_synthetic_pcm_receives_saved_global_and_meeting_hints_with_bounded_nonempty_prefix() {
 let p=super::tests::pool().await;
 sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY); INSERT INTO meetings VALUES ('old'); CREATE TABLE meeting_whisper_vocabulary(meeting_id TEXT PRIMARY KEY,vocabulary TEXT,updated_at TEXT);").execute(&p).await.unwrap();
 let engine=TranscriptionEngine::Whisper(WhisperEngine{calls:Mutex::new(vec![])});
 let pcm:Vec<f32>=(0..1600).map(|i|if i%2==0{0.1}else{-0.1}).collect();
 transcribe_chunk_with_provider(&engine,AudioChunk{sample_rate:16000,data:pcm.clone(),chunk_id:0},None).await.unwrap();
 V::confirm_global(&p,"Ёлка API").await.unwrap(); V::confirm_global(&p,"OpenAPI").await.unwrap();
 let live=V::get_effective(&p,None).await.unwrap();
 transcribe_chunk_with_provider(&engine,AudioChunk{sample_rate:16000,data:pcm.clone(),chunk_id:1},live.as_deref()).await.unwrap();
 V::add_meeting(&p,"old","SRE").await.unwrap();
 let repeated=V::get_effective(&p,Some("old")).await.unwrap();
 transcribe_chunk_with_provider(&engine,AudioChunk{sample_rate:16000,data:pcm.clone(),chunk_id:2},repeated.as_deref()).await.unwrap();
 if let TranscriptionEngine::Whisper(e)=&engine {
   let calls=e.calls.lock().unwrap(); assert!(calls[0].1.is_empty());
   assert_eq!(calls[1].1.iter().map(|c|char::from_u32(*c as u32).unwrap()).collect::<String>(),"Ёлка API, OpenAPI");
   assert_eq!(calls[2].1.iter().map(|c|char::from_u32(*c as u32).unwrap()).collect::<String>(),"SRE, Ёлка API, OpenAPI");
   assert!(calls.iter().all(|(audio,_)|*audio==pcm));
 }
 let tokens=WhisperEngine::tokenize_initial_prompt(&WhisperContext,Some(&"Я".repeat(400))).unwrap();
 assert_eq!(tokens.len(),224); assert!(tokens.iter().all(|t|*t=='Я' as i32));
 assert!(WhisperEngine::tokenize_initial_prompt(&WhisperContext,None).unwrap().is_empty());
 let result=transcribe_chunk_with_provider(&TranscriptionEngine::Parakeet(Parakeet),AudioChunk{sample_rate:16000,data:pcm,chunk_id:3},live.as_deref()).await.unwrap();
 assert_eq!(result.0,"parakeet output");
}
