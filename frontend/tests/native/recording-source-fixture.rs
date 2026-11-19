// Synthetic desktop/store/device adapters. Production source/preferences and
// command setup blocks are inserted by test-recording-sources-portable.py.
extern crate self as tauri;
extern crate self as tauri_plugin_store;
extern crate self as tauri_plugin_dialog;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
pub trait Runtime: Send + Sync + 'static {}
impl Runtime for () {}
pub trait DialogExt {}
#[derive(Default)]
pub struct Store { pub value: Mutex<Option<serde_json::Value>>, pub fail: AtomicBool }
impl Store {
 pub fn get(&self, _: &str)->Option<serde_json::Value>{self.value.lock().unwrap().clone()}
 pub fn set(&self,_:&str,value:serde_json::Value){*self.value.lock().unwrap()=Some(value);}
 pub fn delete(&self,_:&str){*self.value.lock().unwrap()=None;}
 pub fn save(&self)->anyhow::Result<()>{if self.fail.load(Ordering::SeqCst){anyhow::bail!("Synthetic disk failure")}Ok(())}
}
pub struct AppHandle<R:Runtime> { pub store: Arc<Store>, pub marker:std::marker::PhantomData<R> }
impl<R:Runtime> Clone for AppHandle<R> {fn clone(&self)->Self{Self{store:self.store.clone(),marker:std::marker::PhantomData}}}
pub trait StoreExt {fn store(&self,_:&str)->anyhow::Result<Arc<Store>>;}
impl<R:Runtime> StoreExt for AppHandle<R> {fn store(&self,_:&str)->anyhow::Result<Arc<Store>>{Ok(self.store.clone())}}
pub trait Emitter {fn emit<T:serde::Serialize+Clone>(&self,_:&str,_:T)->Result<(),String>{Ok(())}}
impl<R:Runtime> Emitter for AppHandle<R> {}
pub fn app()->AppHandle<()>{AppHandle{store:Arc::new(Store::default()),marker:std::marker::PhantomData}}

%METER_TEST_MODULE%

pub mod audio {
 #[path="%AUDIO%/recording_sources.rs"] pub mod recording_sources;
 #[path="%AUDIO%/recording_mode.rs"] pub mod recording_mode;
 #[path="%WORK%/src/preferences.rs"] pub mod recording_preferences;
 pub mod common {
  static LOCK:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());
  pub async fn acquire_engine_lifecycle_lock()->tokio::sync::MutexGuard<'static,()>{LOCK.lock().await}
 }
 pub mod recording_commands {
  use crate::{AppHandle,Runtime,Emitter};use std::sync::atomic::{AtomicBool,Ordering};use log::info;
  pub use BUSY as IS_RECORDING;
  pub struct MuteManager { pub captured:(bool,bool),pub mic:AtomicBool,pub system:AtomicBool }
  impl MuteManager {
   fn get_state(&self)->&Self{self}
   fn active_capture_sources(&self)->Option<(bool,bool)>{Some(self.captured)}
   fn set_microphone_muted(&self,value:bool){self.mic.store(value,Ordering::SeqCst);}
   fn set_system_audio_muted(&self,value:bool){self.system.store(value,Ordering::SeqCst);}
  }
  pub static RECORDING_MANAGER:std::sync::Mutex<Option<MuteManager>>=std::sync::Mutex::new(None);
  %MUTE_COMMANDS%
  pub static BUSY:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
  pub fn is_recording_active()->bool{BUSY.load(std::sync::atomic::Ordering::SeqCst)}
  pub fn is_recording_stopping()->bool{false}
 }
 pub mod recording_start_sources {
  %SOURCE_VALIDATION%
 }
}

pub mod capture_setup {
 use super::*;
 use anyhow::Result;
 use audio::recording_sources::RecordingSources;
 use audio::recording_preferences::PerAppTarget;
 use log::{info,warn,error};
 pub static RESOLVED: Mutex<Vec<String>>=Mutex::new(Vec::new());
 #[derive(Clone,Debug)]pub struct AudioDevice{pub name:String}
 fn parse_audio_device(name:&str)->Result<AudioDevice>{RESOLVED.lock().unwrap().push(name.into());Ok(AudioDevice{name:name.into()})}
 async fn get_device_and_config(_:&AudioDevice)->Result<()>{Ok(())}
 fn default_input_device()->Result<AudioDevice>{parse_audio_device("default microphone")}
 fn default_output_device()->Result<AudioDevice>{parse_audio_device("default system")}
 pub struct Owner{pub source:String}
 pub struct RecordingManager{pub sources:RecordingSources,pub per_app_enabled:bool,pub per_app_targets:Vec<PerAppTarget>,pub automatic_zoom:bool}
 impl RecordingManager {
  pub fn new()->Self{Self{sources:RecordingSources::default(),per_app_enabled:false,per_app_targets:vec![],automatic_zoom:false}}
  fn set_recordings_folder(&mut self,_:std::path::PathBuf){}
  fn set_per_app_config(&mut self,enabled:bool,targets:Vec<PerAppTarget>){self.per_app_enabled=enabled;self.per_app_targets=targets;}
  fn set_sources(&mut self,sources:RecordingSources){self.sources=sources;}
  fn set_automatic_zoom(&mut self,enabled:bool){self.automatic_zoom=enabled;}
  %PREPARE_PROVENANCE%
  pub fn boundary(&mut self,microphone_device:Option<Arc<AudioDevice>>,system_device:Option<Arc<AudioDevice>>)->Result<(Option<Arc<AudioDevice>>,Option<Arc<AudioDevice>>,bool)>{
   %MANAGER_BOUNDARY%
   Ok((microphone_device,system_device,self.per_app_enabled))
  }
 }
 pub async fn default_start<R:Runtime>(app:AppHandle<R>)->Result<(RecordingManager,Option<Arc<AudioDevice>>,Option<Arc<AudioDevice>>),String>{
  %DEFAULT_SETUP%
  Ok((manager,microphone_device,system_device))
 }
 pub async fn custom_start<R:Runtime>(app:AppHandle<R>,mic_device_name:Option<String>,system_device_name:Option<String>,lifecycle_owner:Option<Owner>)->Result<(RecordingManager,Option<Arc<AudioDevice>>,Option<Arc<AudioDevice>>),String>{
  %CUSTOM_SETUP%
  Ok((manager,mic_device,system_device))
 }
}
#[cfg(test)] #[path="%ROOT%/frontend/tests/native/recording-sources.rs"]mod selection_tests;
#[cfg(test)] #[path="%ROOT%/frontend/tests/native/recording-source-persistence.rs"]mod persistence_tests;
