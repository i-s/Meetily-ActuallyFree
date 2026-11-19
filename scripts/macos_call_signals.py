#!/usr/bin/env python3
"""Issue #7 local metadata journal and deliberately unqualified I/O replay.

No audio/ASR, RPC, network, recording commands or application control. Run tests
on Linux; native collection is reserved for the owner's batched Mac acceptance.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time

APPS = ("zoom", "discord")
STATES = {
    "idle": False, "settings": False, "mic_test": False, "preview": False,
    "waiting": False, "ringing": False, "connected_muted": True,
    "talking": True, "silence": True, "muted": True, "unmuted": True,
    "reconnect": True, "left_app_open": False,
}
REQUIRED_METADATA = ("machine", "macos", "zoom_version", "discord_version",
                     "meetily_head", "permissions", "route", "discord_call_type")


def validate_manifest(data):
    for key in REQUIRED_METADATA:
        if not isinstance(data.get(key), str) or not data[key].strip():
            raise ValueError(f"manifest needs nonempty {key}")
    for app in APPS:
        bundles = data.get("bundles", {}).get(app)
        if not isinstance(bundles, list) or not bundles or any(
                not isinstance(b, str) or not b.strip() for b in bundles):
            raise ValueError(f"manifest needs exact bundles.{app} list")
    pids = data.get("companion_pids")
    if not isinstance(pids, list) or any(type(p) is not int or p <= 0 for p in pids):
        raise ValueError("manifest needs companion_pids list of positive PIDs (may be empty)")
    return data


def signature(snapshot, bundles, excluded_pids):
    """Missing/failed properties are unknown, not inactivity. No name guessing."""
    if snapshot.get("list_status") != 0 or not isinstance(snapshot.get("processes"), list):
        return None
    rows = []
    for row in snapshot["processes"]:
        # Unreadable identity can hide a target helper, including when all other
        # processes appear inactive. Do not convert a partial scan to false.
        status = row.get("status", {})
        if status.get("pid") != 0 or status.get("bundle_id") != 0:
            return None
        if row.get("pid") in excluded_pids:
            continue
        if row.get("bundle_id") in bundles:
            if (status.get("input") != 0 or status.get("output") != 0
                    or type(row.get("input")) is not bool
                    or type(row.get("output")) is not bool):
                return None
            rows.append(row)
    return (any(r["input"] for r in rows), any(r["output"] for r in rows))


class Candidate:
    """Experimental I/O-only rule, NOT a selected lifecycle detector.

    Two consecutive positive samples; 45 s continuous negative observations.
    Unknown or a sampling gap breaks confirmation/grace, never ends a session.
    """
    def __init__(self, start_samples=2, stop_ms=45000, max_gap_ms=1500):
        self.start_samples = start_samples
        self.stop_ms = stop_ms
        self.max_gap_ms = max_gap_ms
        self.active = False
        self.positive = 0
        self.negative_since = None
        self.previous_ms = None

    def update(self, at_ms, signal):
        if self.previous_ms is not None and at_ms - self.previous_ms > self.max_gap_ms:
            self.positive = 0
            self.negative_since = None
        self.previous_ms = at_ms
        if signal is None:
            self.positive = 0
            self.negative_since = None
        elif signal:
            self.negative_since = None
            self.positive += 1
            if not self.active and self.positive >= self.start_samples:
                self.active = True
                return "start"
        else:
            self.positive = 0
            if self.negative_since is None:
                self.negative_since = at_ms
            if self.active and at_ms - self.negative_since >= self.stop_ms:
                self.active = False
                self.negative_since = None
                return "stop"
        return None


def evaluate(events, start_samples=2, stop_ms=45000, max_gap_ms=1500):
    if not events or events[0].get("kind") != "session":
        raise ValueError("journal must begin with session")
    manifest = validate_manifest(events[0]["manifest"])
    states = {app: None for app in APPS}
    trackers = {app: Candidate(start_samples, stop_ms, max_gap_ms) for app in APPS}
    reports = {app: {"states": {}, "transitions": [], "truth_transitions": [],
                     "unknown_samples": 0, "unknown_truth_samples": 0,
                     "false_starts": 0, "false_stops": 0} for app in APPS}
    previous = -1
    # Unmatched truth transitions remain visible instead of being silently
    # paired to a later call. Marker timestamps are manual, not vendor events.
    pending = {app: None for app in APPS}
    for event in events[1:]:
        at = event.get("at_ms")
        if type(at) is not int or at < previous:
            raise ValueError("journal timestamps must be nondecreasing integer milliseconds")
        previous = at
        if event["kind"] == "mark":
            app, state = event["app"], event["state"]
            if app not in APPS or state not in STATES:
                raise ValueError("invalid ground truth marker")
            old = states[app]
            states[app] = state
            if old is not None and STATES[old] != STATES[state]:
                transition = {"at_ms": at, "kind": "start" if STATES[state] else "stop",
                              "observed_delay_ms": None}
                reports[app]["truth_transitions"].append(transition)
                pending[app] = transition
        elif event["kind"] == "sample":
            for app in APPS:
                report = reports[app]
                sig = signature(event["snapshot"], manifest["bundles"][app],
                                set(manifest["companion_pids"]))
                signal = None if sig is None else any(sig)
                state = states[app]
                bucket = report["states"].setdefault(state or "unmarked", {})
                label = "unknown" if sig is None else f"input={int(sig[0])},output={int(sig[1])}"
                bucket[label] = bucket.get(label, 0) + 1
                if sig is None:
                    report["unknown_samples"] += 1
                transition = trackers[app].update(at, signal)
                if transition:
                    truth = None if state is None else STATES[state]
                    false = None if truth is None else (transition == "start") != truth
                    report["transitions"].append({"at_ms": at, "kind": transition,
                                                  "state": state, "false": false})
                    if false:
                        report["false_starts" if transition == "start" else "false_stops"] += 1
                    target = pending[app]
                    if not false and truth is not None and target and target["kind"] == transition:
                        target["observed_delay_ms"] = at - target["at_ms"]
                        pending[app] = None
                if state is None:
                    report["unknown_truth_samples"] += 1
    for report in reports.values():
        by_label = {}
        for state, counts in report["states"].items():
            if state in STATES:
                for label in counts:
                    if label != "unknown":
                        by_label.setdefault(label, set()).add(STATES[state])
        report["ambiguous_signatures"] = sorted(k for k, v in by_label.items() if len(v) > 1)
        report["missing_states"] = sorted(set(STATES) - set(report["states"]))
    return {"qualification": "UNQUALIFIED: I/O is not call lifecycle; physical matrix required",
            "candidate": {"rule": "input OR output", "start_samples": start_samples,
                          "stop_ms": stop_ms, "max_gap_ms": max_gap_ms}, "apps": reports}


def record(args):
    if sys.platform != "darwin":
        raise ValueError("native collection is macOS-only; portable evaluation/tests run anywhere")
    manifest = validate_manifest(json.loads(Path(args.manifest).read_text()))
    probe = str(Path(args.probe).resolve(strict=True))
    start = time.monotonic_ns()
    elapsed = lambda: (time.monotonic_ns() - start) // 1_000_000
    marks = queue.Queue()

    def read_marks():
        for line in sys.stdin:
            marks.put((elapsed(), line.strip()))
        marks.put((elapsed(), "quit"))

    threading.Thread(target=read_marks, daemon=True).start()
    # Refuse overwrite; metadata is private local diagnostic material, not a
    # fixture. One writer serializes samples/marks in timestamp order.
    fd = os.open(args.output, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    with os.fdopen(fd, "w") as output:
        def emit(event):
            output.write(json.dumps(event, ensure_ascii=False, sort_keys=True) + "\n")
            output.flush()
        emit({"kind": "session", "schema": 1, "manifest": manifest,
              "interval_ms": args.interval_ms, "time_basis": "collector monotonic receipt ms"})
        print("Mark actual state: zoom|discord STATE; 'quit' ends. No automatic recording.", file=sys.stderr)
        print("States: " + ", ".join(STATES), file=sys.stderr)
        while elapsed() < args.duration_ms:
            tick = time.monotonic()
            try:
                result = subprocess.run([probe], capture_output=True, text=True, timeout=3, check=True)
                snapshot = json.loads(result.stdout)
            except (subprocess.SubprocessError, ValueError, OSError) as error:
                # No raw stderr/path/UI text in the journal.
                snapshot = {"list_status": "probe_failed", "error_type": type(error).__name__}
            at = elapsed()
            quit_requested = False
            while True:
                try:
                    mark_at, line = marks.get_nowait()
                except queue.Empty:
                    break
                if line == "quit":
                    quit_requested = True
                    continue
                parts = line.split()
                if len(parts) == 2 and parts[0] in APPS and parts[1] in STATES:
                    # Classification time is the collector's serialized receipt;
                    # retain input time separately to expose polling uncertainty.
                    emit({"kind": "mark", "at_ms": at, "input_at_ms": mark_at,
                          "app": parts[0], "state": parts[1]})
                else:
                    print("Invalid marker (not retained)", file=sys.stderr)
            emit({"kind": "sample", "at_ms": at, "snapshot": snapshot,
                  "probe_duration_ms": round((time.monotonic() - tick) * 1000)})
            if quit_requested:
                break
            time.sleep(max(0, args.interval_ms / 1000 - (time.monotonic() - tick)))
        emit({"kind": "end", "at_ms": elapsed()})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    collect = commands.add_parser("record")
    collect.add_argument("--probe", required=True)
    collect.add_argument("--manifest", required=True)
    collect.add_argument("--output", required=True)
    collect.add_argument("--interval-ms", type=int, default=500)
    collect.add_argument("--duration-ms", type=int, default=900000)
    replay = commands.add_parser("evaluate")
    replay.add_argument("journal")
    replay.add_argument("--start-samples", type=int, default=2)
    replay.add_argument("--stop-ms", type=int, default=45000)
    replay.add_argument("--max-gap-ms", type=int, default=1500)
    args = parser.parse_args()
    try:
        if args.command == "record":
            if args.interval_ms < 100 or not 1000 <= args.duration_ms <= 7200000:
                raise ValueError("interval >=100 ms; duration 1000..7200000 ms")
            record(args)
        else:
            if args.start_samples < 1 or args.stop_ms < 0 or args.max_gap_ms < 1:
                raise ValueError("invalid candidate thresholds")
            events = [json.loads(line) for line in Path(args.journal).read_text().splitlines() if line.strip()]
            print(json.dumps(evaluate(events, args.start_samples, args.stop_ms, args.max_gap_ms), indent=2))
    except (ValueError, OSError, KeyError, TypeError) as error:
        parser.exit(2, f"{error}\n")


if __name__ == "__main__":
    main()
