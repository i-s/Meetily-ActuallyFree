use super::*;

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
    ] {
        assert!(args.contains(&serde_json::json!(flag)), "missing {flag}");
    }
    assert!(!args.contains(&serde_json::json!("--model")));
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
