// Test-only synthetic stream boundary. Lifecycle, policy read, serialization,
// generation fences and awaited teardown above are unmodified production code.
static OPEN_STREAMS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static OPEN_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static OPENED: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(0);
mod cpal { pub type Stream = super::FakeStream; }
struct FakeStream;
impl FakeStream {
 fn play(&self)->anyhow::Result<()>{Ok(())}
 fn pause(&self)->anyhow::Result<()>{Ok(())}
}
impl Drop for FakeStream {fn drop(&mut self){OPEN_STREAMS.fetch_sub(1,Ordering::SeqCst);}}
fn default_monitor_devices(kind:Option<&str>)->Vec<String>{vec![kind.unwrap_or("dual").into()]}
fn open_level_stream(_: &str,_:LevelMap,_:Option<&str>)->Result<FakeStream>{
 OPEN_STREAMS.fetch_add(1,Ordering::SeqCst);OPEN_CALLS.fetch_add(1,Ordering::SeqCst);OPENED.add_permits(1);Ok(FakeStream)
}

#[cfg(test)]
mod tests {
 use super::*;
 use crate::audio::recording_preferences::{load_recording_preferences,save_recording_preferences,set_recording_source_enabled};
 use std::future::Future;
 fn profile()->(tempfile::TempDir,crate::AppHandle<()>) {
  let folder=tempfile::tempdir().unwrap();let app=crate::app();
  app.store.set("preferences",serde_json::json!({"save_folder":folder.path(),"auto_save":true,"file_format":"mp4","microphone_enabled":true,"system_audio_enabled":true,"per_app_recording_enabled":false}));
  (folder,app)
 }
 #[tokio::test]
 async fn chosen_app_preview_never_opens_endpoint_audio() {
  let (_folder,app)=profile();
  let mut prefs=load_recording_preferences(&app).await.unwrap();prefs.per_app_recording_enabled=true;
  save_recording_preferences(&app,&prefs).await.unwrap();
  let before=OPEN_CALLS.load(Ordering::SeqCst);
  start_monitoring(app.clone(),vec![],Some("output".into())).await.unwrap();
  // Old command advertises monitoring before spawning its capture thread. Wait
  // for that thread's boundary, rather than racing an immediate counter read.
  if is_monitoring(){OPENED.acquire().await.unwrap().forget();}
  let after=OPEN_CALLS.load(Ordering::SeqCst);
  stop_monitoring().await.unwrap();
  assert_eq!(after,before,"chosen-app output opened endpoint-wide audio");
 }
 #[tokio::test]
 async fn choosing_apps_awaits_endpoint_stream_drop_and_no_target_variant_reopens_it() {
  let (_folder,app)=profile();
  start_monitoring(app.clone(),vec![],Some("output".into())).await.unwrap();
  OPENED.acquire().await.unwrap().forget();
  assert_eq!(OPEN_STREAMS.load(Ordering::SeqCst),1);
  let mut prefs=load_recording_preferences(&app).await.unwrap();
  prefs.per_app_recording_enabled=true;
  save_recording_preferences(&app,&prefs).await.unwrap();
  assert_eq!(OPEN_STREAMS.load(Ordering::SeqCst),0,"endpoint audio continued after chosen-app save");
  for targets in [vec![],vec!["zoom.us"],vec!["Discord"]] {
   prefs.per_app_targets=targets.into_iter().map(|exe|crate::audio::recording_preferences::PerAppTarget{id:exe.into(),name:exe.into(),executable:exe.into(),icon:None}).collect();
   save_recording_preferences(&app,&prefs).await.unwrap();
   let before=OPEN_CALLS.load(Ordering::SeqCst);
   start_monitoring(app.clone(),vec![],Some("output".into())).await.unwrap();
   assert_eq!(OPEN_CALLS.load(Ordering::SeqCst),before);
   assert_eq!(OPEN_STREAMS.load(Ordering::SeqCst),0);
  }
 }
 #[tokio::test]
 async fn excluded_source_and_delayed_cancelled_start_never_reach_stream_open() {
  let (_folder,app)=profile();
  set_recording_source_enabled(app.clone(),"system".into(),false).await.unwrap();
  let before=OPEN_CALLS.load(Ordering::SeqCst);
  start_monitoring(app.clone(),vec![],Some("output".into())).await.unwrap();
  assert_eq!(OPEN_CALLS.load(Ordering::SeqCst),before);
  set_recording_source_enabled(app.clone(),"system".into(),true).await.unwrap();
  let write=crate::audio::recording_preferences::PREFERENCES_WRITE.lock().await;
  let mut delayed=Box::pin(start_monitoring(app.clone(),vec![],Some("output".into())));
  assert!(std::future::poll_fn(|cx|std::task::Poll::Ready(delayed.as_mut().poll(cx).is_pending())).await);
  stop_monitoring().await.unwrap();
  drop(write);delayed.await.unwrap();
  assert_eq!(OPEN_CALLS.load(Ordering::SeqCst),before);
 }
 #[tokio::test]
 async fn unrelated_gain_save_keeps_the_current_valid_preview() {
  let (_folder,app)=profile();
  start_monitoring(app.clone(),vec![],Some("input".into())).await.unwrap();
  let mut prefs=load_recording_preferences(&app).await.unwrap();prefs.mic_gain=1.5;
  save_recording_preferences(&app,&prefs).await.unwrap();
  let active=OPEN_STREAMS.load(Ordering::SeqCst);
  stop_monitoring().await.unwrap();
  assert_eq!(active,1,"unrelated gain edit stopped valid microphone preview");
 }
 #[tokio::test]
 async fn microphone_and_global_preview_still_open_and_stop_acknowledges_drop() {
  let (_folder,app)=profile();
  for kind in ["input","output"]{
   start_monitoring(app.clone(),vec![],Some(kind.into())).await.unwrap();
   OPENED.acquire().await.unwrap().forget();
   assert_eq!(OPEN_STREAMS.load(Ordering::SeqCst),1);
   stop_monitoring().await.unwrap();
   assert_eq!(OPEN_STREAMS.load(Ordering::SeqCst),0);
  }
 }
}
