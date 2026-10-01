use super::*;

#[test]
fn codex_cli_command_removes_parent_session_identity() {
    let cmd = command(Path::new("codex"), &[], Path::new("."));
    let env: std::collections::HashMap<_, _> = cmd.as_std().get_envs().collect();
    for key in [
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
    ] {
        assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None), "{key}");
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
    for text in ["Logged in using an API key - sk-test", "Not logged in", ""] {
        assert!(!chatgpt_login(&Output {
            success: true,
            stdout: String::new(),
            stderr: text.into()
        }));
    }
    assert!(chatgpt_login(&Output {
        success: true,
        stdout: String::new(),
        stderr: "Logged in using ChatGPT\n".into()
    }));
    assert!(!chatgpt_login(&Output {
        success: false,
        stdout: "Logged in using ChatGPT".into(),
        stderr: String::new()
    }));
}

#[cfg(unix)]
fn fixture(contents: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
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
 print('codex-cli test-fixture')
elif sys.argv[1:] == ['exec', '--help']:
 print('--ephemeral --ignore-user-config --ignore-rules')
elif sys.argv[1:] == ['login', 'status']:
 print('Logged in using ChatGPT', file=sys.stderr)
else:
 root.joinpath('invocation.json').write_text(json.dumps({'args':sys.argv[1:], 'prompt':sys.stdin.read(), 'cwd':os.getcwd(), 'api_key_present':'OPENAI_API_KEY' in os.environ or 'CODEX_API_KEY' in os.environ}))
 print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'Итог встречи'}}))
 print(json.dumps({'type':'turn.completed'}))
"##,
    );
    let path = binary.to_str().unwrap();
    let status = probe(Some(path)).await;
    assert!(
        status.installed && status.logged_in && status.error.is_none(),
        "{status:?}"
    );
    let prompt = "Transcript: $(touch should-not-exist); & `echo secret`\n".repeat(4000);
    assert_eq!(
        generate(Some(path), "default", "Answer in Russian", &prompt, None)
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
    let (_dir, binary) = fixture("#!/bin/sh\ncase \"$1 $2\" in\n '--version ') echo codex-test;;\n 'exec --help') echo old-cli;;\n *) exit 1;;\nesac\n");
    let status = probe(binary.to_str()).await;
    assert!(status.installed);
    assert!(!status.logged_in);
    assert!(status.error.unwrap().contains("Update Codex"));
    assert!(!probe(Some("/no/such/codex")).await.installed);
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
            command(&binary, &[], dir.path()),
            Some("prompt".repeat(100_000)),
            Duration::from_secs(30),
            Some(&token)
        )
    );
    assert!(result.err().unwrap().contains("cancelled"));
    assert!(start.elapsed() < Duration::from_secs(3));
    let result = run(
        command(&binary, &[], dir.path()),
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
    let (_dir, binary) = fixture("#!/bin/sh\nif [ \"$1\" = login ]; then echo 'Logged in using ChatGPT'; else echo 'SECRET_TRANSCRIPT sk-private' >&2; exit 1; fi\n");
    let error = generate(binary.to_str(), "default", "system", "prompt", None)
        .await
        .unwrap_err();
    assert!(error.contains("Codex CLI failed"));
    assert!(!error.contains("SECRET") && !error.contains("sk-private"));
}

/// Opt-in CLI contract probe: no real account, transcript, or model inference.
/// Use MEETILY_CODEX_TEST_BINARY to select an already installed native CLI.
#[tokio::test]
#[ignore = "requires an installed Codex CLI; uses only a localhost Responses fixture"]
async fn codex_cli_real_binary_advertises_no_tools_and_accepts_diagnostics() {
    real_binary_contract(false).await;
}

#[tokio::test]
#[ignore = "requires an installed Codex CLI; uses only a localhost Responses fixture"]
async fn codex_cli_real_binary_accepts_recovered_stream_error() {
    real_binary_contract(true).await;
}

async fn real_binary_contract(interrupt_first_stream: bool) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let binary = std::env::var_os("MEETILY_CODEX_TEST_BINARY")
        .map(PathBuf::from)
        .expect("set MEETILY_CODEX_TEST_BINARY to an installed native Codex executable");
    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = async move {
        for attempt in 0..=usize::from(interrupt_first_stream) {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut content_length = None;
            loop {
                let mut line = String::new();
                assert!(socket.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                assert!(
                    !lower.starts_with("authorization:"),
                    "fixture must not receive credentials"
                );
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let count = content_length.expect("CLI must send Content-Length");
            assert!(count < 1024 * 1024);
            let mut body = vec![0; count];
            socket.read_exact(&mut body).await.unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let message = serde_json::json!({"id":"msg_fixture", "type":"message", "role":"assistant", "status":"completed", "content":[{"type":"output_text", "text":"Synthetic summary", "annotations":[]}]});
            let events = [
                serde_json::json!({"type":"response.created", "response":{"id":"resp_fixture", "status":"in_progress", "output":[]}}),
                serde_json::json!({"type":"response.output_item.added", "output_index":0, "item":message}),
                serde_json::json!({"type":"response.output_item.done", "output_index":0, "item":message}),
                serde_json::json!({"type":"response.completed", "response":{"id":"resp_fixture", "status":"completed", "output":[message], "usage":{"input_tokens":1, "output_tokens":1, "total_tokens":2}}}),
            ];
            // A cleanly closed SSE connection without response.completed forces
            // Codex to reconnect and emit its actual retry event.
            let event_count = if interrupt_first_stream && attempt == 0 {
                1
            } else {
                events.len()
            };
            let data: String = events[..event_count]
                .iter()
                .map(|e| format!("data: {e}\n\n"))
                .collect();
            let reply = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}", data.len());
            socket.write_all(reply.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
            assert_eq!(
                request["tools"],
                serde_json::json!([]),
                "CLI advertised tools"
            );
        }
    };
    let provider = format!("model_providers.fixture={{name=\"Local fixture\",base_url=\"http://{address}/v1\",wire_api=\"responses\",requires_openai_auth=false}}");
    let mut args = generation_args("gpt-5.4");
    // Only replace provider routing. Exercise the exact production isolation
    // settings; an empty auth home and cleared environment prevent account use.
    args.pop();
    args.extend(["-c", "model_provider=\"fixture\"", "-c", &provider, "-"]);
    let mut cmd = command(&binary, &args, dir.path());
    cmd.env_clear()
        .env("HOME", dir.path())
        .env("CODEX_HOME", dir.path())
        .env("USERPROFILE", dir.path())
        .env("PATH", binary.parent().unwrap())
        .env("CI", "1");
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        cmd.env("SystemRoot", root);
    }
    let client = run(
        cmd,
        Some("Return a synthetic meeting summary.".into()),
        Duration::from_secs(30),
        None,
    );
    let (_, result) = tokio::time::timeout(Duration::from_secs(35), async {
        tokio::join!(server, client)
    })
    .await
    .expect("local CLI contract probe timed out");
    let output = result.unwrap();
    assert!(output.success, "isolated CLI execution failed");
    if interrupt_first_stream {
        assert!(
            output.stdout.lines().any(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .is_ok_and(|event| event["type"] == "error")
            }),
            "CLI must emit a retry error before recovering"
        );
    }
    assert_eq!(parse_response(&output.stdout).unwrap(), "Synthetic summary");
}
