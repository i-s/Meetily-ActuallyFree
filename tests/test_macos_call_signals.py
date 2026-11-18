"""Portable synthetic fixtures only; no Mac signals or hardware are simulated as evidence."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "signals", Path(__file__).resolve().parents[1] / "scripts/macos_call_signals.py")
S = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(S)


def manifest():
    return {**{key: "synthetic fixture" for key in S.REQUIRED_METADATA},
            "bundles": {"zoom": ["us.zoom.xos"], "discord": ["com.hnc.Discord"]},
            "companion_pids": [99]}


def row(pid=1, bundle="us.zoom.xos", input=False, output=False):
    return {"pid": pid, "bundle_id": bundle, "input": input, "output": output,
            "status": dict.fromkeys(("pid", "bundle_id", "input", "output"), 0)}


def snapshot(*rows):
    return {"list_status": 0, "processes": list(rows)}


def mark(at, state, app="zoom"):
    return {"kind": "mark", "at_ms": at, "app": app, "state": state}


def sample(at, *rows):
    return {"kind": "sample", "at_ms": at, "snapshot": snapshot(*rows)}


def journal(*events):
    return [{"kind": "session", "manifest": manifest()}, *events]


class SignalTests(unittest.TestCase):
    def sig(self, snap):
        return S.signature(snap, ["us.zoom.xos"], {99})

    def test_actual_io_not_process_presence(self):
        self.assertEqual(self.sig(snapshot(row())), (False, False))
        self.assertEqual(self.sig(snapshot(row(input=True))), (True, False))

    def test_exact_bundle_no_substring_match(self):
        self.assertEqual(self.sig(snapshot(row(bundle="not.us.zoom.xos"))), (False, False))

    def test_companion_exclusion(self):
        self.assertEqual(self.sig(snapshot(row(pid=99, input=True))), (False, False))

    def test_helper_pid_rotation_preserves_aggregate(self):
        self.assertEqual(self.sig(snapshot(row(pid=42, input=True))),
                         self.sig(snapshot(row(pid=43, input=True))))

    def test_enum_failure_unknown(self):
        self.assertIsNone(self.sig({"list_status": -1, "processes": []}))

    def test_property_failure_unknown(self):
        r = row(input=True)
        r["status"]["input"] = -1
        self.assertIsNone(self.sig(snapshot(r)))

    def test_unreadable_identity_unknown(self):
        r = row(bundle="")
        r["status"]["bundle_id"] = -1
        self.assertIsNone(self.sig(snapshot(r)))

    def test_missing_boolean_unknown(self):
        self.assertIsNone(self.sig(snapshot(row(input=None))))

    def test_empty_list_not_generic_device_activity(self):
        self.assertEqual(self.sig(snapshot()), (False, False))

    def test_candidate_requires_confirmation(self):
        c = S.Candidate()
        self.assertIsNone(c.update(0, True))
        self.assertEqual(c.update(500, True), "start")
        self.assertIsNone(c.update(1000, True))

    def test_unknown_never_stops(self):
        c = S.Candidate(start_samples=1)
        c.update(0, True)
        self.assertIsNone(c.update(100000, None))
        self.assertTrue(c.active)

    def test_unknown_breaks_start_confirmation(self):
        c = S.Candidate()
        c.update(0, True)
        c.update(500, None)
        self.assertIsNone(c.update(1000, True))

    def test_gap_not_continuous_inactivity(self):
        c = S.Candidate(start_samples=1, stop_ms=1000)
        c.update(0, True)
        c.update(500, False)
        self.assertIsNone(c.update(10000, False))
        self.assertTrue(c.active)

    def test_unknown_resets_end_grace(self):
        c = S.Candidate(start_samples=1, stop_ms=1000)
        c.update(0, True)
        c.update(500, False)
        c.update(1000, None)
        self.assertIsNone(c.update(1500, False))
        self.assertEqual(c.update(2500, False), "stop")

    def test_mic_test_collision_and_false_start_are_visible(self):
        report = S.evaluate(journal(mark(0, "mic_test"), sample(0, row(input=True)),
                            sample(500, row(input=True)), mark(1000, "connected_muted"),
                            sample(1000, row(input=True))))["apps"]["zoom"]
        self.assertEqual(report["false_starts"], 1)
        self.assertEqual(report["ambiguous_signatures"], ["input=1,output=0"])
        self.assertIsNone(report["truth_transitions"][0]["observed_delay_ms"])

    def test_muted_and_reconnect_are_truth_not_signal(self):
        report = S.evaluate(journal(mark(0, "connected_muted"), sample(0, row()),
                            sample(500, row()), mark(1000, "reconnect"), sample(1000, row())))
        self.assertEqual(report["apps"]["zoom"]["transitions"], [])

    def test_latency_and_unmatched_truth(self):
        report = S.evaluate(journal(mark(0, "idle"), sample(0, row()),
                            mark(500, "talking"), sample(500, row(input=True)),
                            sample(1000, row(input=True)), mark(1500, "left_app_open"),
                            sample(1500, row())))["apps"]["zoom"]
        self.assertEqual(report["truth_transitions"][0]["observed_delay_ms"], 500)
        self.assertIsNone(report["truth_transitions"][1]["observed_delay_ms"])

    def test_false_stop_during_silence(self):
        events = [mark(0, "talking"), sample(0, row(input=True)), sample(500, row(input=True)),
                  mark(1000, "silence"), sample(1000, row()), sample(1500, row()), sample(2000, row())]
        report = S.evaluate(journal(*events), stop_ms=1000)["apps"]["zoom"]
        self.assertEqual(report["false_stops"], 1)

    def test_apps_do_not_share_truth_or_signal(self):
        report = S.evaluate(journal(mark(0, "talking"), mark(0, "idle", "discord"),
                            sample(0, row(input=True)), sample(500, row(input=True))))
        self.assertEqual(len(report["apps"]["zoom"]["transitions"]), 1)
        self.assertEqual(report["apps"]["discord"]["transitions"], [])

    def test_unmarked_truth_is_not_scored_as_success(self):
        report = S.evaluate(journal(sample(0, row(input=True)), sample(500, row(input=True))))
        zoom = report["apps"]["zoom"]
        self.assertIsNone(zoom["transitions"][0]["false"])
        self.assertEqual(zoom["unknown_truth_samples"], 2)
        self.assertEqual(len(zoom["missing_states"]), len(S.STATES))

    def test_cli_evaluates_synthetic_journal(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "synthetic.jsonl"
            path.write_text("\n".join(json.dumps(e) for e in journal(
                mark(0, "idle"), sample(0, row()), mark(500, "talking"),
                sample(500, row(input=True)), sample(1000, row(input=True)))))
            result = subprocess.run([sys.executable, "-I", "-B", S.__file__, "evaluate", str(path)],
                                    capture_output=True, text=True, check=True)
            report = json.loads(result.stdout)
            self.assertTrue(report["qualification"].startswith("UNQUALIFIED"))
            self.assertEqual(report["apps"]["zoom"]["truth_transitions"][0]["observed_delay_ms"], 500)

    def test_bad_manifest_rejected(self):
        with self.assertRaises(ValueError):
            S.validate_manifest({})

    def test_reordered_journal_rejected(self):
        with self.assertRaises(ValueError):
            S.evaluate(journal(sample(500, row()), sample(0, row())))


if __name__ == "__main__":
    unittest.main()
