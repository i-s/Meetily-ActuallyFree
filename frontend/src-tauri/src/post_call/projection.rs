//! Materialize immutable post-call evidence into the existing editable summary
//! and task tables. SQLite owns the publication/summary CAS, not the WebView.
use super::{Job, Revision};
use serde_json::{json, Value};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

pub async fn capture(tx: &mut Transaction<'_, Sqlite>, job: &Job) -> Result<(), String> {
    let row: Option<(String,i64,String)> = sqlx::query_as("SELECT action_sync_token,user_edited,status FROM summary_processes WHERE meeting_id=?")
        .bind(&job.meeting_id).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?;
    let (token, eligible) = if let Some((token,edited,status)) = row {
        let owned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM post_call_projections WHERE meeting_id=? AND summary_token=?)")
            .bind(&job.meeting_id).bind(&token).fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
        (Some(token), edited==0 && status.eq_ignore_ascii_case("completed") && owned)
    } else { (None,true) };
    // Capture once per claim. Retrying a callback must not bless a renewed row.
    sqlx::query("INSERT INTO post_call_projection_attempts(meeting_id,epoch,summary_token,eligible) VALUES(?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(&job.meeting_id).bind(job.epoch).bind(token).bind(eligible).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    Ok(())
}
fn text(value: &str) -> String {
    // Evidence/model text stays literal in the normal Markdown editor. It must
    // not create action headings, links or a second participant-agreement block.
    value.replace('\\',"\\\\").chars().flat_map(|c| {
        if "`*_{}[]<>#+!|".contains(c) { vec!['\\',c] } else if c=='\n'||c=='\r' {vec![' ']} else {vec![c]}
    }).collect()
}
struct Task { text:String, owner:Option<String>, due:Option<String>, row:Option<String>, time:Option<f64> }

async fn render(tx:&mut Transaction<'_,Sqlite>, revision:&Revision, raw:&str)->Result<Option<(String,Vec<Task>)>,String>{
    if raw.len()>512_000 { return Ok(None); }
    let value:Value=match serde_json::from_str(raw){Ok(v)=>v,Err(_)=>return Ok(None)};
    let rows:Vec<(String,String,Option<f64>)>=sqlx::query_as("SELECT id,transcript,audio_start_time FROM transcripts WHERE meeting_id=?")
        .bind(&revision.meeting_id).fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
    let notes:Option<String>=sqlx::query_scalar("SELECT notes_markdown FROM meeting_notes WHERE meeting_id=?")
        .bind(&revision.meeting_id).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?.flatten();
    let mut markdown=Vec::new();let mut tasks=Vec::new();
    for (key,title,agreement) in [("summary","Summary",false),("decisions","Decisions",true),("tasks","Tasks",true),("open_questions","Open questions",false)] {
        let Some(claims)=value[key].as_array() else{return Ok(None)};
        if claims.len()>100{return Ok(None)}
        let mut lines=Vec::new();
        for claim in claims {
            let Some(body)=claim["text"].as_str().filter(|s|!s.trim().is_empty()&&s.len()<=8000) else{return Ok(None)};
            let Some(evidence)=claim["evidence"].as_array().filter(|e|!e.is_empty()&&e.len()<=20) else{return Ok(None)};
            let mut anchor=None;let mut personal=false;let mut quotes=Vec::new();
            for e in evidence {
                let Some(quote)=e["quote"].as_str().filter(|s|!s.trim().is_empty()) else{return Ok(None)};
                match e["source"].as_str(){
                    Some("transcript")=>{
                        let Some(row)=rows.iter().find(|r|Some(r.0.as_str())==e["row_id"].as_str()&&r.1.contains(quote)) else{return Ok(None)};
                        if anchor.is_none(){anchor=Some((row.0.clone(),row.2));}
                        quotes.push(format!("Speech evidence: {}",text(quote)));
                    },
                    Some("user_notes") if e["row_id"].is_null()&&notes.as_deref().unwrap_or("").contains(quote)=>{
                        personal=true;quotes.push(format!("Personal Notes (not speech): {}",text(quote)));
                    },
                    _=>return Ok(None),
                }
            }
            if personal&&(agreement||!claim["owner"].is_null()||!claim["deadline"].is_null()||!body.starts_with("Personal note: ")){return Ok(None)}
            let mut supported=Vec::new();
            for key in ["owner","deadline"] {
                if claim[key].is_null(){supported.push(None);continue;}
                let Some(v)=claim[key]["value"].as_str().filter(|s|!s.trim().is_empty()) else{return Ok(None)};
                let Some(q)=claim[key]["quote"].as_str().filter(|q|!q.is_empty()&&q.contains(v)) else{return Ok(None)};
                if !evidence.iter().any(|e|e["source"]=="transcript"&&e["quote"].as_str().is_some_and(|s|s.contains(q))){return Ok(None)}
                supported.push(Some(v.to_string()));
            }
            if key=="tasks" {
                tasks.push(Task{text:body.into(),owner:supported[0].clone(),due:supported[1].clone(),row:anchor.as_ref().map(|a|a.0.clone()),time:anchor.and_then(|a|a.1)});
            } else {
                lines.push(format!("- {}\n  {}",text(body),quotes.join("\n  ")));
            }
        }
        // Tasks are owned rows, not a second auto-extracted Markdown section.
        if key!="tasks" {markdown.push(format!("## {title}\n\n{}",if lines.is_empty(){"No supported items.".into()}else{lines.join("\n\n")}));}
    }
    let Some(uncertainty)=value["uncertainty"].as_array().filter(|a|a.len()<=100) else{return Ok(None)};
    let mut lines=Vec::new();for item in uncertainty{let Some(s)=item.as_str().filter(|s|s.len()<=2000) else{return Ok(None)};lines.push(format!("- {}",text(s)));}
    if !lines.is_empty(){markdown.push(format!("## Uncertainty\n\n{}",lines.join("\n")));}
    Ok(Some((markdown.join("\n\n"),tasks)))
}

pub async fn apply(tx:&mut Transaction<'_,Sqlite>,job:&Job,revision:&Revision,vocabulary:i64,raw:&str)->Result<bool,String>{
    if vocabulary < 0 { return Ok(false); }
    let baseline:Option<(Option<String>,bool)>=sqlx::query_as("SELECT summary_token,eligible FROM post_call_projection_attempts WHERE meeting_id=? AND epoch=?")
        .bind(&job.meeting_id).bind(job.epoch).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?;
    let Some((baseline,true))=baseline else{return Ok(false)};
    let current:Option<(String,i64)>=sqlx::query_as("SELECT action_sync_token,user_edited FROM summary_processes WHERE meeting_id=?")
        .bind(&job.meeting_id).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?;
    if current.as_ref().map(|r|&r.0)!=baseline.as_ref()||current.is_some_and(|r|r.1!=0){return Ok(false)}
    let applied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM post_call_projection_receipts WHERE meeting_id=? AND version_id=? AND transcript_revision=? AND notes_revision=? AND vocabulary_revision=?)")
        .bind(&revision.meeting_id).bind(&revision.version_id).bind(revision.transcript_revision).bind(revision.notes_revision).bind(vocabulary).fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
    if applied{return Ok(false)}
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM post_call_jobs j JOIN transcript_selection s ON s.meeting_id=j.meeting_id WHERE j.meeting_id=? AND j.epoch=? AND j.status='complete' AND j.vocabulary_revision=? AND s.version_id=? AND s.revision=?) AND COALESCE((SELECT revision FROM meeting_notes WHERE meeting_id=?),0)=?")
        .bind(&job.meeting_id).bind(job.epoch).bind(vocabulary).bind(&revision.version_id).bind(revision.transcript_revision).bind(&job.meeting_id).bind(revision.notes_revision).fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
    if !valid||revision.meeting_id!=job.meeting_id{return Ok(false)}
    let Some((markdown,tasks))=render(tx,revision,raw).await? else{return Ok(false)};
    let summary=json!({"markdown":markdown,"post_call":{"source":"post_call_results","job_epoch":job.epoch,"version_id":revision.version_id,"transcript_revision":revision.transcript_revision,"notes_revision":revision.notes_revision,"vocabulary_revision":vocabulary,"artifact_json":raw}});
    sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result,user_edited) VALUES(?,'completed',datetime('now'),datetime('now'),?,0) ON CONFLICT(meeting_id) DO UPDATE SET status='completed',result=excluded.result,updated_at=excluded.updated_at,error=NULL,result_backup=NULL,result_backup_timestamp=NULL,user_edited=0")
        .bind(&job.meeting_id).bind(summary.to_string()).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    sqlx::query("DELETE FROM action_items WHERE meeting_id=? AND source='ai' AND edited=0 AND done=0").bind(&job.meeting_id).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    let mut seen:Vec<String>=sqlx::query_scalar("SELECT lower(trim(text)) FROM action_items WHERE meeting_id=?").bind(&job.meeting_id).fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
    let mut position:i64=sqlx::query_scalar("SELECT COALESCE(MAX(position),-1)+1 FROM action_items WHERE meeting_id=?").bind(&job.meeting_id).fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
    for task in tasks {
        let key=task.text.trim().to_lowercase();if seen.contains(&key){continue;}seen.push(key);
        let person:Option<String>=if let Some(owner)=&task.owner{sqlx::query_scalar("SELECT id FROM people WHERE normalized_name=? ORDER BY id LIMIT 1").bind(owner.trim().to_lowercase()).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?}else{None};
        let id=format!("action-{}",Uuid::new_v4());
        sqlx::query("INSERT INTO action_items(id,meeting_id,text,owner_label,person_id,due_text,done,source,edited,audio_time,transcript_id,position,created_at,updated_at) VALUES(?,?,?,?,?,?,0,'ai',0,?,?,?,datetime('now'),datetime('now'))")
            .bind(&id).bind(&job.meeting_id).bind(task.text).bind(task.owner).bind(person).bind(task.due).bind(task.time).bind(&task.row).bind(position).execute(&mut **tx).await.map_err(|e|e.to_string())?;
        sqlx::query("INSERT INTO action_item_transcript_sources(action_id,version_id,transcript_id,audio_time) VALUES(?,?,?,?)").bind(id).bind(&revision.version_id).bind(task.row).bind(task.time).execute(&mut **tx).await.map_err(|e|e.to_string())?;position+=1;
    }
    sqlx::query("UPDATE meetings SET action_items_source=?,updated_at=datetime('now') WHERE id=?").bind(format!("post-call:{}:{}:{}:{}",revision.version_id,revision.transcript_revision,revision.notes_revision,vocabulary)).bind(&job.meeting_id).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    let token:String=sqlx::query_scalar("SELECT action_sync_token FROM summary_processes WHERE meeting_id=?").bind(&job.meeting_id).fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
    sqlx::query("INSERT INTO post_call_projections(meeting_id,version_id,transcript_revision,notes_revision,vocabulary_revision,job_epoch,summary_token) VALUES(?,?,?,?,?,?,?) ON CONFLICT(meeting_id) DO UPDATE SET version_id=excluded.version_id,transcript_revision=excluded.transcript_revision,notes_revision=excluded.notes_revision,vocabulary_revision=excluded.vocabulary_revision,job_epoch=excluded.job_epoch,summary_token=excluded.summary_token")
        .bind(&job.meeting_id).bind(&revision.version_id).bind(revision.transcript_revision).bind(revision.notes_revision).bind(vocabulary).bind(job.epoch).bind(token).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    sqlx::query("INSERT INTO post_call_projection_receipts(meeting_id,version_id,transcript_revision,notes_revision,vocabulary_revision,job_epoch) VALUES(?,?,?,?,?,?)")
        .bind(&job.meeting_id).bind(&revision.version_id).bind(revision.transcript_revision).bind(revision.notes_revision).bind(vocabulary).bind(job.epoch).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    Ok(true)
}

/// Upgrade/restart recovery is conservative: only a completed, exact current
/// artifact with no canonical row and no prior applied receipt may be projected.
pub async fn recover(pool:&SqlitePool,meeting:Option<&str>)->Result<(),String>{
    let jobs=super::list(pool).await?;
    for job in jobs.into_iter().filter(|j|j.status=="complete"&&j.result_current&&meeting.is_none_or(|m|m==j.meeting_id)){
        let (Some(version),Some(transcript),Some(notes),Some(vocabulary),Some(raw))=(&job.result_version,job.result_transcript_revision,job.result_notes_revision,job.result_vocabulary_revision,&job.result_json) else{continue};
        let mut tx=pool.begin().await.map_err(|e|e.to_string())?;
        sqlx::query("UPDATE meetings SET id=id WHERE id=?").bind(&job.meeting_id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM summary_processes WHERE meeting_id=?)").bind(&job.meeting_id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
        if !exists {
            capture(&mut tx,&job).await?;
            apply(&mut tx,&job,&Revision{meeting_id:job.meeting_id.clone(),version_id:version.clone(),transcript_revision:transcript,notes_revision:notes},vocabulary,raw).await?;
        }
        tx.commit().await.map_err(|e|e.to_string())?;
    }
    Ok(())
}


/// An untouched generated projection is readable only for its exact source
/// tuple. User-owned rows are not hidden merely because the transcript changed.
pub async fn canonical(pool:&SqlitePool,meeting:&str)->Result<Option<Value>,String>{
    recover(pool,Some(meeting)).await?;
    let raw:Option<String>=sqlx::query_scalar("SELECT s.result FROM current_meeting_summaries s JOIN post_call_projections p ON p.meeting_id=s.meeting_id JOIN transcript_selection t ON t.meeting_id=p.meeting_id WHERE s.meeting_id=? AND lower(s.status) IN ('completed','failed','cancelled') AND ((s.user_edited=0 AND t.version_id=p.version_id AND t.revision=p.transcript_revision AND COALESCE((SELECT revision FROM meeting_notes WHERE meeting_id=p.meeting_id),0)=p.notes_revision) OR s.user_edited=1)")
        .bind(meeting).fetch_optional(pool).await.map_err(|e|e.to_string())?.flatten();
    let value:Option<Value>=raw.map(|s|serde_json::from_str(&s).map_err(|e|e.to_string())).transpose()?;
    Ok(value.filter(|v|v.get("post_call").is_some()))
}
