//! Test-only JSON-lines transport to the extracted production command crate.
//! Device/model setup and post-accept save are fixture adapters, NOT Tauri/audio.
use meetily_start_authority::{audio::{recording_commands as commands, recording_ownership as ownership,
    recording_start_authority as authority, common}, meeting_detection, discord_automation, AppHandle};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::{Mutex, atomic::Ordering};
static OUTPUT:Mutex<()>=Mutex::new(());
fn app()->AppHandle<()> { AppHandle(std::marker::PhantomData) }
fn expected(args:&Value)->Result<authority::StartRequest,String>{serde_json::from_value(args["expected"].clone()).map_err(|e|e.to_string())}
async fn command(name:&str,args:Value)->Result<Value,String>{
    match name {
        "get_recording_start_capability"=>Ok(serde_json::to_value(authority::get_recording_start_capability()).unwrap()),
        "is_recording"=>Ok(json!(commands::IS_RECORDING.load(Ordering::SeqCst))),
        "get_recording_state"=>Ok(commands::get_recording_state().await),
        "start_recording_with_devices_and_meeting"=>{
            let lifecycle=args.get("lifecycleSession").and_then(Value::as_str).map(str::to_owned);
            let ticket=args.get("expectedStart").map(|v|serde_json::from_value::<authority::StartRequest>(v.clone()).map_err(|e|e.to_string())).transpose()?;
            let lease=commands::start_recording_with_lifecycle(app(),None,None,None,lifecycle,ticket).await?;
            Ok(serde_json::to_value(lease).unwrap())
        },
        "resolve_automated_recording_start"=>Ok(serde_json::to_value(commands::resolve_automated_recording_start(expected(&args)?).await?).unwrap()),
        "cancel_automated_recording_start"=>Ok(serde_json::to_value(commands::cancel_automated_recording_start(expected(&args)?).await?).unwrap()),
        "stop_recording"=>{
            let args:commands::RecordingArgs=serde_json::from_value(args["args"].clone()).map_err(|e|e.to_string())?;
            Ok(json!(commands::stop_recording(app(),args).await?==commands::StopOutcome::Completed))
        },
        "release_automated_recording"=>{
            let lease=serde_json::from_value(args["expected"].clone()).map_err(|e|e.to_string())?;
            Ok(json!(ownership::release_automated_recording(lease)))
        },
        "set_meeting_automation_enabled"=>{ownership::set_meeting_automation_enabled(args["enabled"].as_bool().unwrap());Ok(Value::Null)},
        "get_meeting_lifecycle_snapshot"=>Ok(serde_json::to_value(meeting_detection::lifecycle::get_meeting_lifecycle_snapshot()).unwrap()),
        "get_discord_automation_status"=>Ok(serde_json::to_value(discord_automation::get_discord_automation_status()).unwrap()),
        "fixture_source"=>{
            let session=args["session_id"].as_str().map(str::to_owned);
            let state=args["state"].as_str().unwrap_or("unknown").to_owned();
            let enabled=args["enabled"].as_bool().unwrap_or(true);
            let qualified=args["qualified"].as_bool().unwrap_or(false);
            if args["source"]=="zoom" {
                meeting_detection::OPT_IN.store(enabled,Ordering::SeqCst);
                meeting_detection::lifecycle::fixture(session,state,qualified);
            } else {
                discord_automation::fixture(discord_automation::Status{enabled,qualified,state,session_id:session,transition:args["transition"].as_str().unwrap_or("none").into()});
            }
            Ok(Value::Null)
        },
        "fixture_reset"=>{
            let _lock=common::acquire_engine_lifecycle_lock().await;
            ownership::claim_stop(None);ownership::set_meeting_automation_enabled(true);
            commands::IS_RECORDING.store(false,Ordering::SeqCst);commands::IS_STOPPING.store(false,Ordering::SeqCst);
            commands::HOLD.store(false,Ordering::SeqCst);commands::FAIL.store(false,Ordering::SeqCst);
            commands::CAPTURES.store(0,Ordering::SeqCst);commands::SAVES.store(0,Ordering::SeqCst);
            Ok(Value::Null)
        },
        "fixture_hold"=>{commands::HOLD.store(true,Ordering::SeqCst);Ok(Value::Null)},
        "fixture_entered"=>{commands::ENTERED.acquire().await.unwrap().forget();Ok(Value::Null)},
        "fixture_release"=>{commands::HOLD.store(false,Ordering::SeqCst);commands::RELEASE.add_permits(1);Ok(Value::Null)},
        "fixture_fail"=>{commands::FAIL.store(args["fail"].as_bool().unwrap(),Ordering::SeqCst);Ok(Value::Null)},
        "fixture_stopping"=>{commands::IS_STOPPING.store(args["stopping"].as_bool().unwrap(),Ordering::SeqCst);Ok(Value::Null)},
        "fixture_counts"=>Ok(json!({"captures":commands::CAPTURES.load(Ordering::SeqCst),"saves":commands::SAVES.load(Ordering::SeqCst)})),
        _=>Err(format!("Unexpected native fixture command {name}")),
    }
}
fn main(){
    let runtime=tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    for line in std::io::stdin().lock().lines(){
        let input:Value=serde_json::from_str(&line.unwrap()).unwrap();
        runtime.spawn(async move{
            let result=command(input["command"].as_str().unwrap(),input["args"].clone()).await;
            let response=match result {Ok(value)=>json!({"id":input["id"],"value":value}),Err(error)=>json!({"id":input["id"],"error":error})};
            let _lock=OUTPUT.lock().unwrap();let mut out=std::io::stdout().lock();
            writeln!(out,"{response}").unwrap();out.flush().unwrap();
        });
    }
}
