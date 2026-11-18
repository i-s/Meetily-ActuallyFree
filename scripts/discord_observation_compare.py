#!/usr/bin/env python3
"""Compare Discord's observation-only export with finite manually marked truth.

Offline, standard library only; no audio, AX, network or application control.
A report never grants qualification. Times are native monotonic milliseconds
relative to journal start, not wall time or inferred Discord event timestamps.
"""
import argparse
from collections import Counter
import json
from pathlib import Path

TRUTH = {
    "idle": False, "settings": False, "mic_test": False, "ringing": False,
    "joined": True, "joined_muted": True, "muted": True, "unmuted": True,
    "silence": True, "channel_changed": True, "device_changed": True,
    "reconnecting": True, "reconnected": True, "left_app_open": False,
    "client_quit": False, "permission_lost": None, "permission_restored": None,
}
REQUIRED = ("idle", "settings", "mic_test", "ringing", "joined_muted", "muted",
            "unmuted", "silence", "left_app_open", "reconnecting", "reconnected",
            "permission_lost", "permission_restored", "client_quit")


def compare(data, tolerance_ms=1500, join_budget_ms=10000, leave_budget_ms=55000):
    if data.get("schema") != 1 or data.get("mode") != "observation_only":
        raise ValueError("expected Discord observation_only schema 1")
    if data.get("scenario") not in ("guild_voice", "dm", "group_dm"):
        raise ValueError("scenario must be guild_voice, dm or group_dm")
    entries = data.get("entries")
    if not isinstance(entries, list) or len(entries) > 4096:
        raise ValueError("invalid or oversized entries")
    previous = -1
    logical = None
    last_truth_at = None
    samples = []
    edges = []
    markers = Counter()
    reasons = Counter()
    counts = Counter()
    violations = []
    for seq, entry in enumerate(entries):
        at = entry.get("at_ms")
        if entry.get("seq") != seq or type(at) is not int or at < previous:
            raise ValueError("entries need ordered seq and monotonic nonnegative at_ms")
        previous = at
        truth = entry.get("truth")
        sample = entry.get("sample")
        if (truth is None) == (sample is None):
            raise ValueError("entry must contain exactly one truth marker or sample")
        if truth is not None:
            if truth not in TRUTH:
                raise ValueError("unknown manual truth marker")
            markers[truth] += 1
            desired = TRUTH[truth]
            if desired is not None:
                if desired != logical:
                    edges.append({"at_ms": at, "truth": truth, "joined": desired})
                logical = desired
            last_truth_at = at
            continue
        start = entry.get("scan_from_ms")
        if type(start) is not int or start < 0 or start > at:
            raise ValueError("sample needs bounded native scan interval")
        if sample.get("qualified") is not False:
            violations.append({"at_ms": at, "error": "qualification_gate_not_false"})
        if sample.get("recording_active") is not False:
            violations.append({"at_ms": at, "error": "recording_active_or_missing"})
        state = sample.get("state")
        if state not in ("active", "inactive", "unknown"):
            raise ValueError("unknown candidate state")
        # Disabled/unsupported is unavailable evidence, never a negative call.
        usable = sample.get("enabled") is True and sample.get("platform_supported") is True
        boundary = last_truth_at is None or start <= last_truth_at + tolerance_ms
        reasons[sample.get("reason", "missing_reason")] += 1
        counts["samples"] += 1
        if not usable:
            counts["disabled_or_unsupported"] += 1
        elif boundary or logical is None:
            counts["truth_boundary_or_unmarked"] += 1
        else:
            counts["comparable"] += 1
            counts["truth_joined" if logical else "truth_not_joined"] += 1
            if state == "unknown": counts["unknown_while_joined" if logical else "unknown_while_not_joined"] += 1
            elif state == "active" and not logical: counts["candidate_false_join"] += 1
            elif state == "inactive" and logical: counts["candidate_false_inactive"] += 1
            else: counts["candidate_matches_truth"] += 1
            if sample.get("transition") == "start" and not logical:
                violations.append({"at_ms": at, "error": "tracker_false_start"})
            if sample.get("transition") == "end" and logical:
                violations.append({"at_ms": at, "error": "tracker_premature_end"})
        samples.append((at, start, sample, usable, boundary))
    results = []
    for index, edge in enumerate(edges):
        # Initial idle establishes baseline, not a requested end transition.
        if index == 0 and not edge["joined"]:
            continue
        end = edges[index + 1]["at_ms"] if index + 1 < len(edges) else previous + 1
        transition = "start" if edge["joined"] else "end"
        budget = join_budget_ms if edge["joined"] else leave_budget_ms
        matches = [at for at, start, sample, usable, _ in samples
                   if usable and start > edge["at_ms"] + tolerance_ms and at < end
                   and sample.get("transition") == transition]
        latency = min(matches) - edge["at_ms"] if matches else None
        results.append({**edge, "expected_transition": transition, "latency_ms": latency,
                        "result": "observed_within_budget" if latency is not None and latency <= budget
                        else "observed_late" if latency is not None
                        else "missing" if end - edge["at_ms"] >= budget + tolerance_ms
                        else "censored_short_interval", "budget_ms": budget})
    return {"schema": 1, "scenario": data["scenario"], "qualification": "not_granted",
            "active_export": bool(data.get("active")), "truncated": bool(data.get("truncated")),
            "tolerance_ms": tolerance_ms, "counts": dict(sorted(counts.items())),
            "unknown_and_signal_reasons": dict(sorted(reasons.items())),
            "manual_markers": dict(sorted(markers.items())),
            "missing_required_markers": sorted(set(REQUIRED) - markers.keys()),
            "transitions_vs_manual_truth": results, "violations": violations,
            "limits": "Manual markers have human reaction delay; scans crossing markers are excluded. Zero violations is not qualification; inspect coverage, Unknown, missing/censored transitions and physical build metadata."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("journal", type=Path)
    parser.add_argument("--tolerance-ms", type=int, default=1500)
    args = parser.parse_args()
    if args.tolerance_ms < 0:
        parser.error("tolerance must be nonnegative")
    if args.journal.stat().st_size > 16 * 1024 * 1024:
        parser.error("journal exceeds 16 MiB")
    result = compare(json.loads(args.journal.read_text(encoding="utf-8")), args.tolerance_ms)
    print(json.dumps(result, indent=2, sort_keys=True))
    # No success status means 'qualified': 0 is only a completed valid comparison.
    return 2 if result["violations"] or result["truncated"] or result["active_export"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
