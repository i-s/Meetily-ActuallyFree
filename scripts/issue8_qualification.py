#!/usr/bin/env python3
"""Portable issue #8 qualification journal.

This tool measures local process/resource and ASR diagnostic output. It never reads
transcript/audio files and never sends anything except the embedded synthetic prompt
to the explicitly selected local Codex CLI. Physical Mac acceptance remains manual.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

SCHEMA_VERSION = 1
SYNTHETIC_PROMPT = """Use only this synthetic text; do not access files or tools.
RU: Встреча начинается в десять часов, обсуждаем план релиза.
EN: The synthetic test asks whether the release plan was discussed.
Mixed: Проверить terminology and answer with the word SYNTHETIC.
Return a short answer containing SYNTHETIC and nothing confidential.
"""


def finite(value, name):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"{name} must be a finite number")
    return float(value)


def read_proc(pid):
    """Return best-effort RSS/swap bytes; unavailable fields are explicit nulls."""
    if sys.platform.startswith("linux"):
        fields = {}
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            key, _, value = line.partition(":")
            if key in ("VmRSS", "VmSwap"):
                fields[key] = int(value.split()[0]) * 1024
        return {"rss_bytes": fields.get("VmRSS"), "swap_bytes": fields.get("VmSwap")}
    ps = shutil.which("ps")
    if not ps:
        return {"rss_bytes": None, "swap_bytes": None}
    result = subprocess.run([ps, "-o", "rss=", "-p", str(pid)], capture_output=True, text=True, check=False)
    rss = int(result.stdout.strip()) * 1024 if result.returncode == 0 and result.stdout.strip().isdigit() else None
    # macOS ps has no portable per-process swap column; report that limitation.
    if rss is None:
        raise ProcessLookupError(pid)
    return {"rss_bytes": rss, "swap_bytes": None}


def system_metrics():
    if sys.platform != "darwin":
        return {"system_swap_bytes": None, "power": "unavailable"}
    swap = subprocess.run(["/usr/sbin/sysctl", "-n", "vm.swapusage"], capture_output=True, text=True, timeout=5)
    used = re.search(r"used\s*=\s*([0-9.]+)([KMG])", swap.stdout)
    power = subprocess.run(["/usr/bin/pmset", "-g", "batt"], capture_output=True, text=True, timeout=5)
    return {"system_swap_bytes": float(used[1]) * {"K": 1024, "M": 1024**2, "G": 1024**3}[used[2]] if used else None,
            "power": "ac" if "AC Power" in power.stdout else "battery" if "Battery Power" in power.stdout else "unavailable"}


def sample(args):
    pid = int(args.pid)
    duration = finite(args.duration, "duration")
    interval = finite(args.interval, "interval")
    if pid <= 0 or not 0 < duration <= 43200 or not 0.01 <= interval <= 60:
        raise ValueError("require pid > 0, duration > 0..43200, interval 0.01..60 seconds")
    started = time.monotonic()
    rows = []
    while True:
        elapsed = time.monotonic() - started
        try:
            metrics = read_proc(pid)
        except (FileNotFoundError, ProcessLookupError):
            rows.append({"elapsed_seconds": elapsed, "process_alive": False, "rss_bytes": None, "swap_bytes": None})
            break
        try:
            system = system_metrics()
        except (OSError, subprocess.TimeoutExpired):
            system = {"system_swap_bytes": None, "power": "unavailable"}
        rows.append({"elapsed_seconds": elapsed, "process_alive": True, **metrics, **system})
        if elapsed >= duration:
            break
        time.sleep(min(interval, max(0, duration - elapsed)))
    rss = [r["rss_bytes"] for r in rows if r["rss_bytes"] is not None]
    swap = [r["swap_bytes"] for r in rows if r["swap_bytes"] is not None]
    return {
        "schema_version": SCHEMA_VERSION, "kind": "issue8_resource_samples",
        "pid": pid, "platform": platform.platform(), "samples": rows,
        "rss_peak_bytes": max(rss) if rss else None,
        "swap_peak_bytes": max(swap) if swap else None,
        "limits": "RSS is process resident memory. macOS per-process swap is unavailable via ps; use Activity Monitor/vm_stat and record it separately.",
    }


def codex(args):
    executable = args.executable or shutil.which("codex")
    if not executable:
        return {"schema_version": SCHEMA_VERSION, "kind": "issue8_codex_qna", "status": "not_run", "reason": "codex executable not found"}
    timeout = finite(args.timeout, "timeout")
    if not 0 < timeout <= 300:
        raise ValueError("timeout must be > 0 and <= 300 seconds")
    command = [executable, "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
               "--skip-git-repo-check", "--sandbox", "read-only",
               "-c", 'model_provider="openai"', "-c", 'forced_login_method="chatgpt"',
               "-c", 'web_search="disabled"', "-c", "features.shell_tool=false", "-"]
    started = time.monotonic()
    try:
        # No repository, attachments or meeting directories in the CLI workspace.
        # Do not inherit API-key routing; only the installed subscription login is allowed.
        env = {k: v for k, v in os.environ.items() if not (k.endswith("API_KEY") or k in ("OPENAI_BASE_URL", "OPENAI_ORG_ID", "OPENAI_PROJECT_ID"))}
        with tempfile.TemporaryDirectory(prefix="meetily-issue8-synthetic-") as cwd:
            login = subprocess.run([executable, "login", "status"], capture_output=True, text=True,
                                   cwd=cwd, env=env, timeout=10)
            if login.returncode != 0 or "using ChatGPT" not in login.stdout + login.stderr:
                return {"schema_version": SCHEMA_VERSION, "kind": "issue8_codex_qna", "status": "not_run",
                        "reason": "subscription login not confirmed; no text sent"}
            version = subprocess.run([executable, "--version"], capture_output=True, text=True,
                                     cwd=cwd, env=env, timeout=10).stdout.strip()
            with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, cwd=cwd, env=env,
                                  start_new_session=True) as process:
                try:
                    stdout, stderr = process.communicate(SYNTHETIC_PROMPT, timeout=timeout)
                except (subprocess.TimeoutExpired, KeyboardInterrupt):
                    os.killpg(process.pid, signal.SIGKILL)
                    process.communicate()
                    raise
                result = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    except subprocess.TimeoutExpired as exc:
        return {"schema_version": SCHEMA_VERSION, "kind": "issue8_codex_qna", "status": "timeout",
                "elapsed_seconds": time.monotonic() - started, "timeout_seconds": float(args.timeout),
                "stdout_bytes": len(exc.stdout or b""), "stderr_bytes": len(exc.stderr or b""),
                "prompt_sha256": hashlib.sha256(SYNTHETIC_PROMPT.encode()).hexdigest(),
                "limits": "The response is intentionally not saved; only synthetic prompt was sent."}
    output = result.stdout or ""
    return {"schema_version": SCHEMA_VERSION, "kind": "issue8_codex_qna",
            "status": "passed" if result.returncode == 0 and "SYNTHETIC" in output.upper() else "failed",
            "cli_version": version, "auth": "chatgpt_subscription", "returncode": result.returncode, "elapsed_seconds": time.monotonic() - started,
            "within_60_seconds": time.monotonic() - started <= 60.0,
            "stdout_sha256": hashlib.sha256(output.encode()).hexdigest(),
            "stdout_bytes": len(output.encode()), "stderr_bytes": len((result.stderr or "").encode()),
            "prompt_sha256": hashlib.sha256(SYNTHETIC_PROMPT.encode()).hexdigest(),
            "limits": "Only the embedded synthetic prompt was sent. A successful CLI response does not qualify network/battery recording or private transcript handling."}


def events(args):
    path = Path(args.path)
    with path.open() as log:
        lines = (line for line in log)
        return summarize_events(lines)


def summarize_events(lines):
    records = []
    for line_no, line in enumerate(lines, 1):
        marker = "MEETILY_ISSUE8 "
        if marker not in line:
            continue
        raw = line.split(marker, 1)[1].strip()
        try:
            item = json.loads(raw)
        except json.JSONDecodeError as exc:
            raise ValueError(f"line {line_no}: invalid diagnostic JSON: {exc}")
        if not isinstance(item, dict):
            raise ValueError(f"line {line_no}: diagnostic must be an object")
        if item.get("kind") in ("asr_chunk", "asr_queued"):
            chunk_id = item.get("chunk_id")
            if isinstance(chunk_id, bool) or not isinstance(chunk_id, int) or chunk_id < 0:
                raise ValueError(f"line {line_no}: require nonnegative integer chunk_id")
        if item.get("kind") == "asr_chunk":
            if not isinstance(item.get("outcome"), str):
                raise ValueError(f"line {line_no}: require outcome string")
            for field in ("total_seconds", "queue_seconds", "processing_seconds"):
                if field in item and finite(item[field], field) < 0:
                    raise ValueError(f"line {line_no}: {field} cannot be negative")
        if item.get("kind") in ("asr_chunk", "asr_queued", "asr_drained"):
            records.append(item)
    chunks = [x for x in records if x.get("kind") == "asr_chunk"]
    bad = [x for x in chunks if x.get("outcome") not in ("emitted", "empty")]
    totals = [finite(x["total_seconds"], "total_seconds") for x in chunks if "total_seconds" in x]
    queued = [x.get("chunk_id") for x in records if x.get("kind") == "asr_queued"]
    finished = [x.get("chunk_id") for x in chunks]
    drains = [x for x in records if x.get("kind") == "asr_drained"]
    queued_counts, finished_counts = Counter(queued), Counter(finished)
    duplicate_enqueues = sorted(i for i, count in queued_counts.items() if count > 1)
    duplicate_completions = sorted(i for i, count in finished_counts.items() if count > 1)
    # Counter differences retain multiplicity: one completion cannot satisfy two enqueues.
    unfinished = sorted((queued_counts - finished_counts).elements())
    unexpected = sorted((finished_counts - queued_counts).elements())
    identities_match = queued_counts == finished_counts and not duplicate_enqueues and not duplicate_completions
    incomplete_timing = [{"chunk_id": x["chunk_id"], "missing_fields": [
        field for field in ("total_seconds", "queue_seconds", "processing_seconds") if field not in x
    ]} for x in chunks]
    incomplete_timing = [x for x in incomplete_timing if x["missing_fields"]]
    return {"schema_version": SCHEMA_VERSION, "kind": "issue8_asr_observations",
            "chunks": len(chunks), "outcomes": {k: sum(x.get("outcome") == k for x in chunks) for k in sorted({x.get("outcome") for x in chunks})},
            "max_dispatch_to_completion_seconds": max(totals) if totals else None,
            "max_queue_seconds": max((x["queue_seconds"] for x in chunks if "queue_seconds" in x), default=None),
            "max_processing_seconds": max((x["processing_seconds"] for x in chunks if "processing_seconds" in x), default=None),
            "queued_chunks": len(queued), "unfinished_chunk_ids": unfinished,
            "unexpected_chunk_ids": unexpected, "duplicate_enqueue_ids": duplicate_enqueues,
            "duplicate_completion_ids": duplicate_completions, "identities_match": identities_match,
            "duplicate_completions": len(finished) - len(set(finished)),
            "stage_timing_complete": bool(chunks) and not incomplete_timing,
            "incomplete_stage_timing": incomplete_timing,
            "drain_observed": bool(drains),
            "drain_counts_match": len(drains) == 1 and identities_match and drains[0].get("queued") == drains[0].get("completed") == len(chunks) == len(queued),
            "failed_or_unknown_chunks": len(bad),
            "status": "observed" if chunks else "no_diagnostic_chunks",
            "limits": "This is dispatcher/VAD-worker timing, not speech-to-text or WebView latency. A 90-minute physical run with recording-relative landmarks is required for acceptance."}


def qualification(args):
    """Fail closed on an incomplete manual physical-run journal; never infer a pass."""
    data = json.loads(args.path.read_text())
    if data.get("schema_version") != 1:
        raise ValueError("require schema_version 1")
    missing, failures = [], []
    post_stop_displays = 0

    def timing(item, field, context):
        if item.get(field) is None:
            missing.append(f"{context}: missing {field}")
            return None
        try:
            return finite(item[field], field)
        except ValueError:
            failures.append(f"{context}: {field} must be finite")
            return None

    if data.get("physical_acceptance") != "measured":
        missing.append("physical_acceptance must be measured, not a template")
    if not data.get("configuration_decision"):
        missing.append("configuration decision/reasons")
    if not data.get("issue6_evidence"):
        missing.append("issue #6 physical evidence and limits")
    runs = data.get("runs", [])
    if not isinstance(runs, list):
        raise ValueError("runs must be an array")
    for machine in ("M4 Pro/48 GB", "M1 Pro/32 GB"):
        for power in ("ac", "battery"):
            selected = [r for r in runs if r.get("machine") == machine and r.get("power") == power]
            key = f"{machine}/{power}"
            if len(selected) != 1:
                missing.append(f"{key}: exactly one 90-minute run required")
                continue
            run = selected[0]
            for field in ("app_sha", "macos_version", "asr_model", "model_sha256", "model_parameters", "corpus_sha256", "resource_evidence", "audio_integrity_evidence", "asr_events_evidence"):
                if not run.get(field):
                    missing.append(f"{key}: {field}")
            duration = finite(run.get("duration_seconds"), "duration_seconds")
            if duration < 5400:
                failures.append(f"{key}: duration < 90 minutes")
            if run.get("audio_integrity") is not True:
                missing.append(f"{key}: verified audio integrity during Q&A and failure")
            if run.get("queue_drained") is not True:
                missing.append(f"{key}: queue drain/count verification")
            for field in ("rss_peak_bytes", "system_swap_peak_bytes"):
                if run.get(field) is None:
                    missing.append(f"{key}: {field}")
                elif finite(run[field], field) < 0:
                    raise ValueError(f"{field} cannot be negative")
            landmarks = run.get("landmarks", [])
            if not landmarks or {x.get("language") for x in landmarks} != {"ru", "en", "mixed"}:
                missing.append(f"{key}: RU/EN/mixed speech/display landmarks")
            drain = timing(run, "drain_completed_seconds", key)
            if drain is not None and drain < duration:
                failures.append(f"{key}: drain completion must be at/after recording stop")
            valid_speech = []
            for index, mark in enumerate(landmarks):
                context = f"{key}/landmark[{index}]"
                speech = timing(mark, "speech_end_seconds", context)
                display = timing(mark, "display_seconds", context)
                if speech is not None:
                    if not 0 <= speech <= duration:
                        failures.append(f"{context}: speech end outside recording [0,duration]")
                    else:
                        valid_speech.append(speech)
                if display is not None:
                    if display < 0 or (speech is not None and display < speech):
                        failures.append(f"{context}: display must be nonnegative and not precede speech")
                    # Post-stop rendering is legitimate tail evidence, not concurrent activity.
                    phase = "post_stop" if display > duration else "recording"
                    if phase == "post_stop":
                        post_stop_displays += 1
                        if mark.get("display_phase") != "post_stop":
                            missing.append(f"{context}: post-stop display requires display_phase=post_stop")
                    if "display_phase" in mark and mark["display_phase"] != phase:
                        failures.append(f"{context}: display_phase inconsistent with recording stop")
                if speech is not None and display is not None and display - speech > 10:
                    failures.append(f"{key}: speech-to-display > 10 seconds")
            if valid_speech and (min(valid_speech) > 60 or max(valid_speech) < duration - 60):
                missing.append(f"{key}: beginning/end landmarks (also sample throughout run)")
            questions = run.get("questions", [])
            if not questions:
                missing.append(f"{key}: concurrent subscription Codex Q&A")
            for index, question in enumerate(questions):
                if question.get("synthetic_only") is not True or question.get("auth") != "chatgpt_subscription":
                    failures.append(f"{key}: text/auth boundary not satisfied")
                if question.get("during_asr") is not True:
                    missing.append(f"{key}: Q&A not observed during ASR")
                if question.get("status") != "passed":
                    failures.append(f"{key}: Q&A failed/unavailable; retry required")
                context = f"{key}/question[{index}]"
                elapsed = timing(question, "elapsed_seconds", context)
                start = timing(question, "start_seconds", context)
                end = timing(question, "end_seconds", context)
                if elapsed is not None:
                    if elapsed < 0:
                        failures.append(f"{context}: Q&A elapsed must be nonnegative")
                    elif elapsed > 60:
                        failures.append(f"{key}: Q&A > 60 seconds")
                if start is not None and end is not None:
                    if not 0 <= start <= end <= duration:
                        failures.append(f"{context}: concurrent Q&A requires 0 <= start <= end <= duration")
                    if elapsed is not None and not math.isclose(end - start, elapsed, rel_tol=0, abs_tol=0.05):
                        failures.append(f"{context}: Q&A interval and elapsed disagree (>0.05 seconds)")
    return {"schema_version": 1, "kind": "issue8_qualification_review",
            "status": "failed" if failures else "blocked" if missing else "manual_review_required",
            "missing_evidence": missing, "failed_targets": failures,
            "post_stop_display_landmarks": post_stop_displays,
            "limits": "Structural checks only. Independently review original audio, corpus/model comparison, resource timeline, concurrent ASR, power evidence and configuration decision. This tool never grants physical acceptance."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("sample", help="sample a running app process")
    p.add_argument("pid"); p.add_argument("--duration", type=float, default=60); p.add_argument("--interval", type=float, default=1)
    p = sub.add_parser("codex", help="run the synthetic-text Codex CLI check")
    p.add_argument("--executable"); p.add_argument("--timeout", type=float, default=60)
    p = sub.add_parser("events", help="summarize MEETILY_ISSUE8 JSONL log lines")
    p.add_argument("path", type=Path)
    p = sub.add_parser("qualification", help="check physical-run journal for missing evidence/failed targets")
    p.add_argument("path", type=Path)
    args = parser.parse_args()
    try: result = {"sample": sample, "codex": codex, "events": events, "qualification": qualification}[args.command](args)
    except (OSError, ValueError) as exc:
        parser.error(str(exc))
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))

if __name__ == "__main__":
    main()
