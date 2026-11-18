//! Production queue publication → canonical summary/actions → normal library/export.
use crate::database::repositories::{library,notes_document};
use crate::post_call::store::{self,Revision};
use crate::transcript_versions as versions;
use serde_json::{json,Value};
use sqlx::SqlitePool;
async fn db()->SqlitePool {
 let p=SqlitePool::connect("sqlite::memory:").await.unwrap();
 for m in crate::migrations::MIGRATIONS{sqlx::raw_sql(m).execute(&p).await.unwrap();}
 sqlx::raw_sql("INSERT INTO meetings(id,title,created_at,updated_at,folder_path) VALUES('m','Meeting','2026','2026','/synthetic'); INSERT INTO transcripts(id,meeting_id,transcript,timestamp,audio_start_time,audio_end_time,speaker) VALUES('line','m','We agreed Alice will send report Friday','00:01',1,2,'Alice');").execute(&p).await.unwrap();
 notes_document::write(&p,"meeting:m","Personal idea",None,0).await.unwrap();
 versions::selected(&p,"m").await.unwrap();p
}
fn result()->String {json!({"summary":[{"text":"Report discussed","evidence":[{"source":"transcript","row_id":"line","quote":"send report"}],"owner":null,"deadline":null}],"decisions":[],"tasks":[{"text":"Send report","evidence":[{"source":"transcript","row_id":"line","quote":"Alice will send report Friday"}],"owner":{"value":"Alice","quote":"Alice will send report Friday"},"deadline":{"value":"Friday","quote":"Friday"}}],"open_questions":[],"uncertainty":["No further commitments"]}).to_string()}
async fn claim(p:&SqlitePool)->(store::Job,Revision){
 let mut tx=p.begin().await.unwrap();store::enqueue(&mut tx,"m").await.unwrap();tx.commit().await.unwrap();
 let j=store::claim(p).await.unwrap().unwrap();store::bind_vocabulary(p,&j,0).await.unwrap();
 let mut tx=p.begin().await.unwrap();store::projection::capture(&mut tx,&j).await.unwrap();tx.commit().await.unwrap();
 let s=versions::selected(p,"m").await.unwrap();(j,Revision{meeting_id:"m".into(),version_id:s.version_id,transcript_revision:s.revision,notes_revision:1})
}
#[tokio::test]
async fn bridge_publication_reaches_normal_search_export_tasks_and_preserves_notes(){
 let p=db().await;let(j,r)=claim(&p).await;
 store::publish(&p,&j,&r,0,&result()).await.unwrap();
 let export=library::export_snapshot(&p,"m",1).await.unwrap();
 let normal=crate::database::repositories::summary::SummaryProcessesRepository::get_summary_data_for_meeting(&p,"m").await.unwrap().expect("normal summary reader missing");
 assert_eq!(normal.result,export.summary);
 let summary:Value=serde_json::from_str(export.summary.as_ref().expect("canonical summary missing")).unwrap();
 assert!(summary["markdown"].as_str().unwrap().contains("Report discussed"));assert_eq!(summary["post_call"]["job_epoch"],j.epoch);
 assert_eq!(summary["post_call"]["notes_revision"],1);assert_eq!(export.actions.len(),1);assert_eq!(export.actions[0].text,"Send report");
 assert_eq!(export.notes.markdown,"Personal idea");
 assert!(library::search(&p,"Report discussed",10,false).await.unwrap().iter().any(|x|x.kind=="summary"));
 let anchor:(String,f64)=sqlx::query_as("SELECT transcript_id,audio_time FROM action_items WHERE meeting_id='m'").fetch_one(&p).await.unwrap();assert_eq!(anchor,("line".into(),1.0));
 assert_eq!(store::results(&p,"m").await.unwrap()[0].result_json,result());
}

#[tokio::test]
async fn bridge_renews_ownership_noops_for_manual_or_replaced_rows_and_preserves_artifact(){
 for writer in ["manual","same-row","reinsert","legacy"]{
  let p=db().await;
  if writer=="legacy"{sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result,user_edited) VALUES('m','completed','2026','2026',?,0)").bind(json!({"markdown":"Legacy kept"}).to_string()).execute(&p).await.unwrap();}
  let(j,r)=claim(&p).await;
  if writer!="legacy"{
   sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result,user_edited) VALUES('m','completed','2026','2026',?,?)").bind(json!({"markdown":"New owner"}).to_string()).bind(if writer=="manual"{1}else{0}).execute(&p).await.unwrap();
   if writer=="reinsert"{sqlx::query("DELETE FROM summary_processes WHERE meeting_id='m'").execute(&p).await.unwrap();sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES('m','completed','2026','2026',?)").bind(json!({"markdown":"New owner"}).to_string()).execute(&p).await.unwrap();}
  }
  sqlx::query("INSERT INTO action_items(id,meeting_id,text,done,source,edited,audio_time,transcript_id,position,created_at,updated_at) VALUES('manual-task','m','Curated task',1,'user',1,1,'line',0,'2026','2026')").execute(&p).await.unwrap();
  let before:String=sqlx::query_scalar("SELECT result FROM summary_processes WHERE meeting_id='m'").fetch_one(&p).await.unwrap();
  store::publish(&p,&j,&r,0,&result()).await.unwrap();store::projection::recover(&p,None).await.unwrap();
  let export=library::export_snapshot(&p,"m",1).await.unwrap();assert_eq!(export.summary,Some(before));assert_eq!(export.actions.len(),1);assert_eq!(export.actions[0].text,"Curated task");
  assert_eq!(store::results(&p,"m").await.unwrap().len(),1);
 }
}
#[tokio::test]
async fn bridge_same_owned_row_renewal_during_refresh_cannot_overwrite_manual_or_new_job(){
 let p=db().await;let(j,r)=claim(&p).await;store::publish(&p,&j,&r,0,&result()).await.unwrap();
 store::control(&p,"m","refresh").await.unwrap();let j=store::claim(&p).await.unwrap().unwrap();store::bind_vocabulary(&p,&j,1).await.unwrap();
 let mut tx=p.begin().await.unwrap();store::projection::capture(&mut tx,&j).await.unwrap();tx.commit().await.unwrap();
 let before:String=sqlx::query_scalar("SELECT result FROM summary_processes WHERE meeting_id='m'").fetch_one(&p).await.unwrap();
 sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at) VALUES('m','PENDING','same','same') ON CONFLICT(meeting_id) DO UPDATE SET status='PENDING',result=result").execute(&p).await.unwrap();
 sqlx::query("UPDATE summary_processes SET status='completed',result=? WHERE meeting_id='m'").bind(&before).execute(&p).await.unwrap();
 store::publish(&p,&j,&r,1,&result()).await.unwrap();
 assert_eq!(library::export_snapshot(&p,"m",1).await.unwrap().summary,Some(before));assert_eq!(store::results(&p,"m").await.unwrap().len(),2);
}
#[tokio::test]
async fn bridge_retry_upgrade_recovery_and_stale_selection_notes_never_resurrect_results(){
 let p=db().await;let(j,r)=claim(&p).await;
 // Simulate retained completed pre-bridge artifact; no captured attempt exists.
 sqlx::query("DELETE FROM post_call_projection_attempts").execute(&p).await.unwrap();
 store::publish(&p,&j,&r,0,&result()).await.unwrap();
 store::projection::recover(&p,Some("m")).await.unwrap();
 assert!(library::export_snapshot(&p,"m",1).await.unwrap().summary.is_some());
 let count:i64=sqlx::query_scalar("SELECT count(*) FROM action_items").fetch_one(&p).await.unwrap();
 store::projection::recover(&p,None).await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM action_items").fetch_one(&p).await.unwrap(),count);
 versions::correct_text(&p,"m","line","Edited speech",r.transcript_revision).await.unwrap();
 assert!(library::export_snapshot(&p,"m",1).await.unwrap().summary.is_none());
 let archived:Option<String>=sqlx::query_scalar("SELECT summary_json FROM transcript_versions WHERE kind='corrections' ORDER BY created_at DESC LIMIT 1").fetch_one(&p).await.unwrap();assert!(archived.unwrap().contains("Report discussed"));
 store::projection::recover(&p,None).await.unwrap();assert!(library::export_snapshot(&p,"m",1).await.unwrap().summary.is_none());
 assert_eq!(store::results(&p,"m").await.unwrap().len(),1);
 let p=db().await;let(j,r)=claim(&p).await;store::publish(&p,&j,&r,0,&result()).await.unwrap();
 notes_document::write(&p,"meeting:m","Changed personal idea",None,1).await.unwrap();
 assert!(library::export_snapshot(&p,"m",2).await.unwrap().summary.is_none());
 assert!(!library::search(&p,"Report discussed",10,false).await.unwrap().iter().any(|x|x.kind=="summary"));
 assert_eq!(store::results(&p,"m").await.unwrap().len(),1);
}
#[tokio::test]
async fn bridge_failure_rolls_back_job_artifact_summary_tasks_and_marker(){
 let p=db().await;let(j,r)=claim(&p).await;
 sqlx::raw_sql("CREATE TRIGGER bridge_failure BEFORE INSERT ON action_items BEGIN SELECT RAISE(ABORT,'synthetic task failure'); END;").execute(&p).await.unwrap();
 assert!(store::publish(&p,&j,&r,0,&result()).await.is_err());
 assert!(store::results(&p,"m").await.unwrap().is_empty());
 assert_eq!(store::list(&p).await.unwrap()[0].status,"running");
 assert!(library::export_snapshot(&p,"m",1).await.unwrap().summary.is_none());
 sqlx::raw_sql("DROP TRIGGER bridge_failure").execute(&p).await.unwrap();
 store::publish(&p,&j,&r,0,&result()).await.unwrap();assert!(library::export_snapshot(&p,"m",1).await.unwrap().summary.is_some());
}

#[tokio::test]
async fn bridge_manual_summary_and_task_edits_survive_refresh_and_original_artifact_stays_immutable(){
 let p=db().await;let(j,r)=claim(&p).await;let original=result();store::publish(&p,&j,&r,0,&original).await.unwrap();
 let id:String=sqlx::query_scalar("SELECT id FROM action_items WHERE meeting_id='m'").fetch_one(&p).await.unwrap();
 crate::database::repositories::action_item::ActionItemsRepository::update(&p,&id,"Curated report wording",None,None,None,true).await.unwrap();
 crate::database::repositories::summary::SummaryProcessesRepository::update_meeting_summary(&p,"m",&json!({"markdown":"Manual summary wording"}),true).await.unwrap();
 store::control(&p,"m","refresh").await.unwrap();let j=store::claim(&p).await.unwrap().unwrap();store::bind_vocabulary(&p,&j,1).await.unwrap();
 let mut tx=p.begin().await.unwrap();store::projection::capture(&mut tx,&j).await.unwrap();tx.commit().await.unwrap();
 store::publish(&p,&j,&r,1,&original).await.unwrap();
 let export=library::export_snapshot(&p,"m",1).await.unwrap();assert!(export.summary.unwrap().contains("Manual summary wording"));assert_eq!(export.actions[0].text,"Curated report wording");assert!(export.actions[0].done);
 assert_eq!(store::results(&p,"m").await.unwrap().len(),2);assert_eq!(store::results(&p,"m").await.unwrap()[1].result_json,original);
}
#[tokio::test]
async fn bridge_raw_selected_edits_hide_old_projection_from_normal_reads_and_action_context(){
 let p=db().await;let(j,r)=claim(&p).await;store::publish(&p,&j,&r,0,&result()).await.unwrap();
 sqlx::query("UPDATE transcripts SET transcript='New canonical words' WHERE id='line'").execute(&p).await.unwrap();
 assert!(library::export_snapshot(&p,"m",1).await.unwrap().summary.is_none());
 assert!(crate::database::repositories::summary::SummaryProcessesRepository::get_summary_data_for_meeting(&p,"m").await.unwrap().is_none());
 assert!(crate::database::repositories::action_sync::context(&p,"m").await.unwrap().is_none());
}

#[tokio::test]
async fn bridge_user_edited_provenance_is_retained_on_detail_reload_even_after_notes_change(){
 let p=db().await;let(j,r)=claim(&p).await;store::publish(&p,&j,&r,0,&result()).await.unwrap();
 let mut value=store::projection::canonical(&p,"m").await.unwrap().unwrap();value["markdown"]=json!("Manual annotated projection");value["user_edited"]=json!(true);
 crate::database::repositories::summary::SummaryProcessesRepository::update_meeting_summary(&p,"m",&value,true).await.unwrap();
 notes_document::write(&p,"meeting:m","New private notes",None,1).await.unwrap();
 assert_eq!(store::projection::canonical(&p,"m").await.unwrap().unwrap()["markdown"],"Manual annotated projection");
 assert!(library::export_snapshot(&p,"m",2).await.unwrap().summary.unwrap().contains("Manual annotated projection"));
}

#[tokio::test]
async fn bridge_pending_manual_summary_save_after_notes_change_has_a_row_to_commit(){
 let p=db().await;let(j,r)=claim(&p).await;store::publish(&p,&j,&r,0,&result()).await.unwrap();
 notes_document::write(&p,"meeting:m","Notes changed while summary draft waits",None,1).await.unwrap();
 assert!(library::export_snapshot(&p,"m",2).await.unwrap().summary.is_none());
 crate::database::repositories::summary::SummaryProcessesRepository::update_meeting_summary(&p,"m",&json!({"markdown":"Preserved manual draft"}),true).await.unwrap();
 assert!(library::export_snapshot(&p,"m",2).await.unwrap().summary.unwrap().contains("Preserved manual draft"));
}
