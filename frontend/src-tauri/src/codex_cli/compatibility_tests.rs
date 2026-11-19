use super::*;

const HELP: &str = "--ephemeral --ignore-user-config --ignore-rules --skip-git-repo-check --sandbox --json --color --model --config";

fn capable(version: &str, help: &str, reject_parse: bool, login: &str) -> (tempfile::TempDir, PathBuf) {
    fixture(&format!(r#"#!/usr/bin/python3 -I
import sys, json
from pathlib import Path
args = sys.argv[1:]
root = Path(__file__).parent
if args == ['--version']:
 print({version:?})
elif args == ['exec', '--help']:
 print({help:?})
elif '--help' in args:
 assert args[0] == 'exec' and args[-1] == '--help'
 assert 'features.shell_tool=false' in args and 'forced_login_method="chatgpt"' in args
 root.joinpath('parser-checked').touch()
 sys.exit({reject_parse} == True)
elif args == ['login', 'status']:
 root.joinpath('login-checked').touch()
 print({login:?})
else:
 root.joinpath('meeting-stdin').write_text(sys.stdin.read())
 print('{{"type":"item.completed","item":{{"type":"agent_message","text":"Synthetic answer"}}}}')
 print('{{"type":"turn.completed"}}')
"#, reject_parse = if reject_parse { "True" } else { "False" }))
}

#[tokio::test]
async fn compatibility_versions_are_metadata_and_full_parser_runs_before_meeting() {
    for version in ["codex-cli 0.167.2", "codex-cli 0.170.0", "codex-cli 0.999.0"] {
        let (dir, binary) = capable(version, HELP, false, "Logged in using ChatGPT");
        let home = dir.path().join("auth");
        let status = probe(binary.to_str(), &home).await;
        assert_eq!(status.readiness, "ready", "{status:?}");
        assert_eq!(status.version.as_deref(), Some(version));
        assert!(dir.path().join("parser-checked").exists(), "help alone is not a parser check");
        assert!(!dir.path().join("meeting-stdin").exists(), "status must not send stdin");
        assert_eq!(generate(binary.to_str(), &home, "default", "synthetic", "SYNTHETIC_MEETING", None).await.unwrap(), "Synthetic answer");
    }
}

#[tokio::test]
async fn compatibility_missing_or_rejected_option_blocks_login_and_meeting() {
    for version in ["codex-cli 0.167.2", "codex-cli 0.170.0"] {
        for (help, rejected, reason) in [(HELP.replace("--json ", "--json-lookalike "), false, "--json"), (HELP.into(), true, "argument") ] {
            let (dir, binary) = capable(version, &help, rejected, "Logged in using ChatGPT");
            let home = dir.path().join("auth");
            let status = probe(binary.to_str(), &home).await;
            assert_eq!(status.readiness, "unsupported", "{status:?}");
            assert!(status.installed);
            assert!(status.error.unwrap().contains(reason));
            assert!(generate(binary.to_str(), &home, "default", "private", "PRIVATE_MEETING", None).await.is_err());
            assert!(!dir.path().join("meeting-stdin").exists());
            assert!(!dir.path().join("login-checked").exists());
        }
    }
}

#[tokio::test]
async fn compatibility_signed_out_is_not_incompatible_and_never_sends_meeting() {
    for version in ["codex-cli 0.167.2", "codex-cli 0.170.0"] {
        for login in ["Not logged in", "Logged in using an API key"] {
            let (dir, binary) = capable(version, HELP, false, login);
            let home = dir.path().join("auth");
            let status = probe(binary.to_str(), &home).await;
            assert_eq!(status.readiness, "signed_out");
            assert!(status.error.is_none(), "recognized authentication state: {status:?}");
            assert!(generate(binary.to_str(), &home, "default", "private", "PRIVATE_MEETING", None).await.is_err());
            assert!(!dir.path().join("meeting-stdin").exists());
        }
    }
}

#[tokio::test]
async fn login_probe_failures_do_not_request_sign_in() {
    for (after_parser, login_body, expected_state, expected_reason) in [
        ("Path(os.environ['CODEX_HOME'], 'AGENTS.override.md').touch()", "pass", "blocked", "AGENTS"),
        ("Path(__file__).unlink()", "pass", "error", "Cannot start Codex CLI"),
        ("pass", "print('PRIVATE_DETAIL', file=sys.stderr); sys.exit(2)", "error", "login status"),
        ("pass", "print('unknown authentication mode')", "error", "login status"),
        ("pass", "import time; time.sleep(12)", "error", "did not respond"),
    ] {
        let (dir, binary) = fixture(&format!(r#"#!/usr/bin/python3 -I
import os, sys
from pathlib import Path
args = sys.argv[1:]
if args == ['--version']: print('codex-cli 0.170.0')
elif args == ['exec', '--help']: print({HELP:?})
elif '--help' in args: {after_parser}
elif args == ['login', 'status']: {login_body}
else: Path(__file__).with_suffix('.sent').touch()
"#));
        let status = probe(binary.to_str(), &dir.path().join("auth")).await;
        assert_eq!(status.readiness, expected_state, "{status:?}");
        assert!(status.installed && !status.logged_in);
        let reason = status.error.unwrap();
        assert!(reason.contains(expected_reason), "{reason}");
        assert!(!reason.contains("PRIVATE_DETAIL"));
        assert!(!binary.with_extension("sent").exists());
    }
}

#[tokio::test]
async fn compatibility_child_home_is_scratch_not_personal_home() {
    let (dir, binary) = fixture("#!/usr/bin/python3 -I\nimport os,json\nprint(json.dumps({'home':os.environ['HOME'],'profile':os.environ.get('USERPROFILE'),'cwd':os.getcwd()}))\n");
    let output = run(command(&binary, &[], dir.path(), &dir.path().join("auth")), None, PROBE_TIMEOUT, None).await.unwrap();
    let value: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(value["home"], value["cwd"]);
    assert_eq!(value["profile"], value["cwd"]);
}
