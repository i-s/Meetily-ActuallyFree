use super::*;

#[test]
fn codex_cli_command_removes_parent_session_identity() {
    let cmd = command(
        Path::new("codex"),
        &[],
        Path::new("."),
        Path::new("/synthetic/auth"),
    );
    let env: std::collections::HashMap<_, _> = cmd.as_std().get_envs().collect();
    for key in [
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
    ] {
        assert!(
            env.get(std::ffi::OsStr::new(key)).is_none(),
            "{key} must not be explicitly inherited after env_clear"
        );
    }
}

#[test]
fn codex_cli_nonfatal_warnings_and_plan_metadata_allow_completed_answers() {
    // Real 0.159.0-alpha.3 emits item.error for configuration warnings, even
    // when the response completes successfully without calling any tools.
    let events = r#"{"type":"item.completed","item":{"type":"error","message":"`[features].memory_tool` is deprecated. Use `[features].memories` instead."}}
{"type":"item.started","item":{"type":"todo_list","items":[]}}
{"type":"item.updated","item":{"type":"todo_list","items":[]}}
{"type":"item.completed","item":{"type":"todo_list","items":[]}}
{"type":"item.completed","item":{"type":"agent_message","text":"Summary"}}
{"type":"turn.completed"}"#;
    assert_eq!(parse_response(events).unwrap(), "Summary");
    assert!(parse_response(&events.replace("turn.completed", "turn.failed")).is_err());
    assert!(parse_response(&format!(
        "{events}\n{{\"type\":\"error\",\"message\":\"private\"}}"
    ))
    .is_err());
}

#[test]
fn codex_cli_accepts_only_recovered_stream_errors() {
    let retry = r#"{"type":"error","message":"Reconnecting... 1/5 (private diagnostic)"}"#;
    let answer = r#"{"type":"item.completed","item":{"type":"agent_message","text":"Summary"}}"#;
    let completed = r#"{"type":"turn.completed"}"#;
    assert_eq!(
        parse_response(&format!("{retry}\n{answer}\n{completed}")).unwrap(),
        "Summary"
    );
    for events in [
        retry.to_owned(),
        format!("{retry}\n{answer}"),
        format!("{retry}\n{answer}\n{{\"type\":\"turn.failed\"}}"),
        format!("{answer}\n{completed}\n{retry}"),
    ] {
        let error = parse_response(&events).unwrap_err();
        assert!(!error.contains("private"));
    }
}

#[test]
fn codex_cli_rejects_execution_items_in_every_lifecycle_phase() {
    for kind in [
        "command_execution",
        "file_change",
        "mcp_tool_call",
        "collab_tool_call",
        "web_search",
    ] {
        for phase in ["item.started", "item.updated", "item.completed"] {
            let events = format!("{{\"type\":\"{phase}\",\"item\":{{\"type\":\"{kind}\",\"text\":\"private\"}}}}\n{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"Summary\"}}}}\n{{\"type\":\"turn.completed\"}}");
            let error = parse_response(&events).unwrap_err();
            assert!(error.contains("tool"), "{phase}/{kind}: {error}");
            assert!(!error.contains("private"));
        }
    }
}

#[test]
fn codex_cli_unknown_items_fail_without_claiming_tool_use() {
    let events = r#"{"type":"item.completed","item":{"type":"future_item","text":"private"}}
{"type":"item.completed","item":{"type":"agent_message","text":"Summary"}}
{"type":"turn.completed"}"#;
    let error = parse_response(events).unwrap_err();
    assert!(error.contains("unsupported item"), "{error}");
    assert!(!error.contains("private"));
}

#[test]
fn codex_cli_response_requires_completion_and_uses_last_agent_message() {
    let events = "{\"type\":\"thread.started\"}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"reasoning\",\"text\":\"private reasoning\"}}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Answer in Russian: да\"}}\n{\"type\":\"turn.completed\"}";
    assert_eq!(parse_response(events).unwrap(), "Answer in Russian: да");
    assert!(parse_response("{\"type\":\"turn.completed\"}").is_err());
    assert!(parse_response(
        "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"partial\"}}"
    )
    .is_err());
    assert!(parse_response(&format!("{events}\n{{\"type\":\"turn.failed\"}}")).is_err());
    assert!(parse_response("not JSON").is_err());
}

#[test]
fn codex_cli_auth_never_accepts_api_key_or_failed_status() {
    for text in ["Logged in using an API key - sk-test", "Not logged in"] {
        assert_eq!(chatgpt_login(&Output {
            success: true,
            stdout: String::new(),
            stderr: text.into()
        }), Ok(false));
    }
    assert_eq!(chatgpt_login(&Output {
        success: true,
        stdout: String::new(),
        stderr: "Logged in using ChatGPT\n".into()
    }), Ok(true));
    assert!(chatgpt_login(&Output {
        success: false,
        stdout: "Logged in using ChatGPT".into(),
        stderr: String::new()
    }).is_err());
}

#[cfg(unix)]
fn fixture(contents: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
    let binary = dir.path().join("codex fixture with spaces");
    std::fs::write(&binary, contents).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, binary)
}

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_production_probe_and_generation_use_existing_login_and_stdin() {
    let (dir, binary) = fixture(
        r##"#!/usr/bin/python3
import sys, json, os
from pathlib import Path
root = Path(__file__).parent
if sys.argv[1:] == ['--version']:
 print('codex-cli 0.167.2')
elif sys.argv[1] == 'exec' and '--help' in sys.argv:
 print('--ephemeral --ignore-user-config --ignore-rules --skip-git-repo-check --sandbox --json --color --model --config')
elif sys.argv[1:] == ['login', 'status']:
 print('Logged in using ChatGPT', file=sys.stderr)
else:
 root.joinpath('invocation.json').write_text(json.dumps({'args':sys.argv[1:], 'prompt':sys.stdin.read(), 'cwd':os.getcwd(), 'api_key_present':'OPENAI_API_KEY' in os.environ or 'CODEX_API_KEY' in os.environ}))
 print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'Итог встречи'}}))
 print(json.dumps({'type':'turn.completed'}))
"##,
    );
    let path = binary.to_str().unwrap();
    let status = probe(Some(path), &dir.path().join("auth")).await;
    assert!(
        status.installed && status.logged_in && status.error.is_none(),
        "{status:?}"
    );
    let prompt = "Transcript: $(touch should-not-exist); & `echo secret`\n".repeat(4000);
    assert_eq!(
        generate(
            Some(path),
            &dir.path().join("auth"),
            "default",
            "Answer in Russian",
            &prompt,
            None
        )
        .await
        .unwrap(),
        "Итог встречи"
    );
    let invocation: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("invocation.json")).unwrap())
            .unwrap();
    let args = invocation["args"].as_array().unwrap();
    for flag in [
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "read-only",
        "features.shell_tool=false",
        "features.unified_exec=false",
        "features.apply_patch_freeform=false",
        "features.goals=false",
        "tools.experimental_request_user_input.enabled=false",
        "features.code_mode_host=false",
        "features.skill_mcp_dependency_install=false",
        "features.skill_search=false",
        "features.skip_host_skill_discovery=true",
        "skills.include_instructions=false",
        "skills.bundled.enabled=false",
    ] {
        assert!(args.contains(&serde_json::json!(flag)), "missing {flag}");
    }
    assert!(!args.contains(&serde_json::json!("--model")));
    assert!(!args.contains(&serde_json::json!("features.memory_tool=false")));
    assert!(!args
        .iter()
        .any(|a| a.as_str().unwrap_or("").contains("Transcript:")));
    assert!(invocation["prompt"].as_str().unwrap().contains(&prompt));
    assert_eq!(invocation["api_key_present"], false);
    assert!(
        !Path::new(invocation["cwd"].as_str().unwrap()).exists(),
        "scratch directory must be removed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_status_reports_old_cli_and_unsigned_cli() {
    let (dir, binary) = fixture("#!/bin/sh\ncase \"$1 $2\" in\n '--version ') echo 'codex-cli 0.167.2';;\n 'exec --help') echo old-cli;;\n *) exit 1;;\nesac\n");
    let status = probe(binary.to_str(), &dir.path().join("auth")).await;
    assert!(status.installed);
    assert!(!status.logged_in);
    assert!(status.error.unwrap().contains("Update Codex"));
    assert!(
        !probe(Some("/no/such/codex"), &dir.path().join("auth"))
            .await
            .installed
    );
}

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_timeout_and_cancel_kill_descendants_and_close_pipes() {
    let (dir, binary) = fixture("#!/bin/sh\n(sleep 1; touch \"$0.marker\") &\nsleep 30\n");
    let token = CancellationToken::new();
    let cancel = token.clone();
    let start = std::time::Instant::now();
    let (_, result) = tokio::join!(
        async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel.cancel();
        },
        run(
            command(&binary, &[], dir.path(), &dir.path().join("auth")),
            Some("prompt".repeat(100_000)),
            Duration::from_secs(30),
            Some(&token)
        )
    );
    assert!(result.err().unwrap().contains("cancelled"));
    assert!(start.elapsed() < Duration::from_secs(3));
    let result = run(
        command(&binary, &[], dir.path(), &dir.path().join("auth")),
        None,
        Duration::from_millis(30),
        None,
    )
    .await;
    assert!(result.err().unwrap().contains("did not respond"));
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!binary
        .with_file_name("codex fixture with spaces.marker")
        .exists());
}

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_nonzero_exit_does_not_leak_diagnostics() {
    let (dir, binary) = fixture("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'codex-cli 0.167.2'; elif [ \"$1\" = exec ] && case \" $* \" in *\" --help \"*) true;; *) false;; esac; then echo '--ephemeral --ignore-user-config --ignore-rules --skip-git-repo-check --sandbox --json --color --model --config'; elif [ \"$1\" = login ]; then echo 'Logged in using ChatGPT'; else echo 'SECRET_TRANSCRIPT sk-private' >&2; exit 1; fi\n");
    let error = generate(
        binary.to_str(),
        &dir.path().join("auth"),
        "default",
        "system",
        "prompt",
        None,
    )
    .await
    .unwrap_err();
    assert!(error.contains("Codex CLI failed"));
    assert!(!error.contains("SECRET") && !error.contains("sk-private"));
}

#[cfg(unix)]
#[path = "compatibility_tests.rs"]
mod compatibility_tests;

#[path = "real_cli_tests.rs"]
mod real_cli_tests;

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_does_not_inherit_workspace_tool_credentials_or_injection_environment() {
    // Serial portable harness: this test owns these synthetic environment names.
    std::env::set_var("MEETILY_SYNTHETIC_SECRET", "must-not-leave-parent");
    std::env::set_var("OPENAI_BASE_URL", "https://invalid.invalid");
    std::env::set_var("NODE_OPTIONS", "--no-warnings");
    let (dir, binary) = fixture("#!/usr/bin/python3\nimport os,json\nprint(json.dumps({k:os.environ[k] for k in ['MEETILY_SYNTHETIC_SECRET','OPENAI_BASE_URL','NODE_OPTIONS'] if k in os.environ}))\n");
    let output = run(
        command(&binary, &[], dir.path(), &dir.path().join("auth")),
        None,
        Duration::from_secs(2),
        None,
    )
    .await
    .unwrap();
    for key in [
        "MEETILY_SYNTHETIC_SECRET",
        "OPENAI_BASE_URL",
        "NODE_OPTIONS",
    ] {
        std::env::remove_var(key);
    }
    assert_eq!(output.stdout.trim(), "{}");
    let args = generation_args("default");
    assert!(args.contains(&"model_provider=\"openai\""));
}

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_blocks_both_global_agents_names_and_symlinked_auth_home_before_any_process() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for name in ["AGENTS.md", "AGENTS.override.md"] {
        let (dir, binary) = fixture("#!/bin/sh\ntouch \"$0.spawned\"\n");
        let home = dir.path().join("dedicated");
        prepare_auth_home(&home).await.unwrap();
        assert_eq!(home.metadata().unwrap().permissions().mode() & 0o777, 0o700);
        std::fs::write(home.join(name), "OWNED_SYNTHETIC_SENTINEL").unwrap();
        let status = probe(binary.to_str(), &home).await;
        assert_eq!(status.readiness, "blocked");
        let error = generate(
            binary.to_str(),
            &home,
            "default",
            "synthetic",
            "SYNTHETIC_MEETING",
            None,
        )
        .await
        .err()
        .unwrap();
        assert!(error.contains("AGENTS"));
        assert!(!binary
            .with_file_name("codex fixture with spaces.spawned")
            .exists());
    }
    let dir = tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
    std::fs::create_dir(dir.path().join("shared")).unwrap();
    symlink(dir.path().join("shared"), dir.path().join("dedicated-link")).unwrap();
    assert!(prepare_auth_home(&dir.path().join("dedicated-link"))
        .await
        .is_err());
    assert!(prepare_auth_home(Path::new("relative-home")).await.is_err());
    assert!(prepare_auth_home(&dir.path().join("dedicated-link/child"))
        .await
        .is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_rechecks_personalization_after_preflight_and_rejects_missing_capabilities() {
    let (dir, binary) = fixture(
        r#"#!/usr/bin/python3
import sys,os
from pathlib import Path
if sys.argv[1:] == ['--version']:
 print('codex-cli 0.167.2')
 Path(os.environ['CODEX_HOME']).joinpath('AGENTS.override.md').write_text('LATE_SYNTHETIC_PERSONALIZATION')
else:
 Path(__file__).with_suffix('.spawned').write_text('must not run')
"#,
    );
    let home = dir.path().join("auth");
    let error = generate(
        binary.to_str(),
        &home,
        "default",
        "synthetic",
        "SYNTHETIC_MEETING",
        None,
    )
    .await
    .err()
    .unwrap();
    assert!(error.contains("AGENTS"));
    assert!(!binary.with_extension("spawned").exists());
    let (dir, binary) = fixture("#!/bin/sh\necho 'codex-cli 0.999.0'\n");
    let status = probe(binary.to_str(), &dir.path().join("auth")).await;
    assert_eq!(status.readiness, "unsupported");
    assert!(!status.logged_in);
}

#[cfg(unix)]
#[tokio::test]
async fn codex_cli_output_overflow_terminates_without_waiting_for_timeout() {
    let (dir, binary) = fixture("#!/usr/bin/python3\nimport sys,time\nsys.stdout.write('x'*(1024*1024+100));sys.stdout.flush();time.sleep(30)\n");
    let started = std::time::Instant::now();
    let result = run(
        command(&binary, &[], dir.path(), &dir.path().join("auth")),
        None,
        Duration::from_secs(10),
        None,
    )
    .await;
    assert!(result.err().unwrap().contains("size limit"));
    assert!(started.elapsed() < Duration::from_secs(2));
}
