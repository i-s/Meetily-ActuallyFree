"""Portable tests for issue #8's measurement and privacy boundaries."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/issue8_qualification.py"
spec = importlib.util.spec_from_file_location("issue8_qualification", SCRIPT)
qualification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualification)


class Issue8Qualification(unittest.TestCase):
    def test_events_are_text_free_and_report_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "app.log"
            path.write_text("""info MEETILY_ISSUE8 {\"kind\":\"asr_chunk\",\"chunk_id\":1,\"capture_source\":\"system\",\"audio_duration_seconds\":1.0,\"total_seconds\":2.5,\"outcome\":\"emitted\"}\nMEETILY_ISSUE8 {\"kind\":\"asr_chunk\",\"chunk_id\":2,\"capture_source\":\"microphone\",\"audio_duration_seconds\":1.0,\"total_seconds\":1.2,\"outcome\":\"failed\"}\n""")
            result = qualification.events(type("Args", (), {"path": path})())
        self.assertEqual(result["chunks"], 2)
        self.assertEqual(result["failed_or_unknown_chunks"], 1)
        self.assertAlmostEqual(result["max_dispatch_to_completion_seconds"], 2.5)
        self.assertNotIn("text", result)

    def test_codex_uses_only_synthetic_stdin_and_does_not_save_response(self):
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / "codex"
            fake.write_text("""#!/bin/sh
if [ "$1" = login ]; then echo 'Logged in using ChatGPT'; exit 0; fi
if [ "$1" = --version ]; then echo 'test-cli'; exit 0; fi
cat >/dev/null
printf 'SYNTHETIC answer\n'
""")
            fake.chmod(0o755)
            result = qualification.codex(type("Args", (), {"executable": str(fake), "timeout": 5})())
        self.assertEqual(result["status"], "passed")
        self.assertNotIn("SYNTHETIC answer", json.dumps(result))
        self.assertIn("prompt_sha256", result)

    def test_codex_timeout_is_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / "codex"
            fake.write_text("#!/bin/sh\nif [ \"$1\" = login ]; then echo 'Logged in using ChatGPT'; exit 0; fi\nsleep 1\n")
            fake.chmod(0o755)
            result = qualification.codex(type("Args", (), {"executable": str(fake), "timeout": 0.01})())
        self.assertEqual(result["status"], "timeout")

    def test_resource_sample_rejects_nonpositive_values(self):
        with self.assertRaises(ValueError):
            qualification.sample(type("Args", (), {"pid": "1", "duration": 0, "interval": 1})())

    def test_missing_evidence_is_blocked_not_passed(self):
        path = SCRIPT.parent.parent / "docs/fixtures/issue8-qualification.example.json"
        result = qualification.qualification(type("Args", (), {"path": path})())
        self.assertEqual(result["status"], "blocked")
        self.assertGreaterEqual(len(result["missing_evidence"]), 4)

    def test_missing_and_duplicate_completions_and_drain(self):
        rows = [
            {"kind": "asr_queued", "chunk_id": 1},
            {"kind": "asr_queued", "chunk_id": 2},
            {"kind": "asr_chunk", "chunk_id": 1, "outcome": "empty", "total_seconds": 3},
            {"kind": "asr_chunk", "chunk_id": 1, "outcome": "empty", "total_seconds": 3},
            {"kind": "asr_drained", "queued": 2, "completed": 2},
        ]
        result = qualification.summarize_events("MEETILY_ISSUE8 " + json.dumps(x) for x in rows)
        self.assertEqual(result["unfinished_chunk_ids"], [2])
        self.assertEqual(result["duplicate_completions"], 1)
        self.assertEqual(result["duplicate_completion_ids"], [1])
        self.assertFalse(result["drain_counts_match"])
        self.assertFalse(result["identities_match"])

    def test_nonfinite_diagnostic_clock_is_rejected(self):
        with self.assertRaises(ValueError):
            qualification.summarize_events(['MEETILY_ISSUE8 {"kind":"asr_chunk","total_seconds":NaN}'])

    def test_resource_sampling_observes_running_process(self):
        result = qualification.sample(type("Args", (), {"pid": str(__import__("os").getpid()), "duration": 0.02, "interval": 0.01})())
        self.assertGreaterEqual(len(result["samples"]), 2)
        self.assertTrue(result["samples"][0]["process_alive"])


    def journal(self):
        runs = []
        for machine in ("M4 Pro/48 GB", "M1 Pro/32 GB"):
            for power in ("ac", "battery"):
                run = dict(machine=machine, power=power, duration_seconds=5400,
                           audio_integrity=True, queue_drained=True, rss_peak_bytes=1,
                           system_swap_peak_bytes=0, drain_completed_seconds=5402,
                           landmarks=[dict(language=language, speech_end_seconds=t, display_seconds=t+1)
                                      for language, t in (("ru", 10), ("en", 2700), ("mixed", 5390))],
                           questions=[dict(synthetic_only=True, auth="chatgpt_subscription",
                                           during_asr=True, status="passed", elapsed_seconds=5,
                                           start_seconds=100, end_seconds=105)])
                for key in ("app_sha", "macos_version", "asr_model", "model_sha256", "model_parameters",
                            "corpus_sha256", "resource_evidence", "audio_integrity_evidence", "asr_events_evidence"):
                    run[key] = "synthetic test placeholder, not evidence"
                runs.append(run)
        return dict(schema_version=1, physical_acceptance="measured", issue6_evidence="test",
                    configuration_decision="test", runs=runs)

    def review_journal(self, data):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "journal.json"
            path.write_text(json.dumps(data))
            return qualification.qualification(type("Args", (), {"path": path})())

    def test_full_journal_never_automatically_grants_acceptance(self):
        data = self.journal()
        self.assertEqual(self.review_journal(data)["status"], "manual_review_required")
        data["runs"][0]["questions"][0]["elapsed_seconds"] = 61
        data["runs"][0]["questions"][0]["end_seconds"] = 161
        data["runs"][0]["landmarks"][0]["display_seconds"] = 30
        result = self.review_journal(data)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(result["failed_targets"]), 2)

    def test_review_reproduction_out_of_recording_clocks_are_not_clean(self):
        data = self.journal()
        for run in data["runs"]:
            for mark, speech in zip(run["landmarks"], (-100, 6000, 9000)):
                mark.update(speech_end_seconds=speech, display_seconds=speech + 1)
            run["questions"][0]["elapsed_seconds"] = -1
        self.assertEqual(self.review_journal(data)["status"], "failed")

    def test_question_requires_valid_concurrent_interval_despite_during_asr(self):
        for changes in ({"start_seconds": None}, {"end_seconds": None},
                        {"start_seconds": -1}, {"end_seconds": 5401},
                        {"start_seconds": 110, "end_seconds": 105},
                        {"elapsed_seconds": -1}, {"elapsed_seconds": float("inf")},
                        {"elapsed_seconds": float("nan")}, {"end_seconds": 106}):
            with self.subTest(changes=changes):
                data = self.journal()
                data["runs"][0]["questions"][0].update(changes)
                self.assertNotEqual(self.review_journal(data)["status"], "manual_review_required")

    def test_speech_clock_bounds_and_nonfinite_values(self):
        for speech in (-1, 5401, float("inf"), float("nan"), None):
            with self.subTest(speech=speech):
                data = self.journal()
                data["runs"][0]["landmarks"][0]["speech_end_seconds"] = speech
                self.assertNotEqual(self.review_journal(data)["status"], "manual_review_required")

    def test_post_stop_display_and_drain_are_separate(self):
        data = self.journal()
        data["runs"][0]["landmarks"][-1].update(speech_end_seconds=5400, display_seconds=5401,
                                                display_phase="post_stop")
        result = self.review_journal(data)
        self.assertEqual(result["status"], "manual_review_required")
        self.assertEqual(result["post_stop_display_landmarks"], 1)
        data["runs"][0]["landmarks"][-1]["display_phase"] = "recording"
        self.assertEqual(self.review_journal(data)["status"], "failed")
        data = self.journal()
        data["runs"][0]["drain_completed_seconds"] = 5399
        self.assertEqual(self.review_journal(data)["status"], "failed")

    def test_exact_id_reconciliation_rejects_duplicate_enqueue_substitution(self):
        rows = [{"kind": "asr_queued", "chunk_id": i} for i in (1, 1)]
        rows += [dict(kind="asr_chunk", chunk_id=i, outcome="emitted", total_seconds=3,
                      queue_seconds=1, processing_seconds=2) for i in (1, 2)]
        rows.append(dict(kind="asr_drained", queued=2, completed=2))
        result = qualification.summarize_events("MEETILY_ISSUE8 " + json.dumps(x) for x in rows)
        self.assertFalse(result["drain_counts_match"])
        self.assertFalse(result["identities_match"])
        self.assertEqual(result["duplicate_enqueue_ids"], [1])
        self.assertEqual(result["unexpected_chunk_ids"], [2])
        self.assertEqual(result["unfinished_chunk_ids"], [1])

    def test_stage_timing_completeness_is_independent_of_matching_ids(self):
        rows = [dict(kind="asr_queued", chunk_id=1),
                dict(kind="asr_chunk", chunk_id=1, outcome="emitted", total_seconds=3),
                dict(kind="asr_drained", queued=1, completed=1)]
        result = qualification.summarize_events("MEETILY_ISSUE8 " + json.dumps(x) for x in rows)
        self.assertTrue(result["drain_counts_match"])
        self.assertTrue(result["identities_match"])
        self.assertFalse(result["stage_timing_complete"])
        self.assertEqual(result["incomplete_stage_timing"], [
            {"chunk_id": 1, "missing_fields": ["queue_seconds", "processing_seconds"]}])

    def test_missing_total_and_stage_durations_are_all_reported(self):
        rows = [dict(kind="asr_queued", chunk_id=1),
                dict(kind="asr_chunk", chunk_id=1, outcome="empty"),
                dict(kind="asr_drained", queued=1, completed=1)]
        result = qualification.summarize_events("MEETILY_ISSUE8 " + json.dumps(x) for x in rows)
        self.assertTrue(result["identities_match"])
        self.assertFalse(result["stage_timing_complete"])
        self.assertEqual(result["incomplete_stage_timing"][0]["missing_fields"],
                         ["total_seconds", "queue_seconds", "processing_seconds"])

    def test_complete_single_session_has_exact_id_and_stage_matches(self):
        rows = [dict(kind="asr_queued", chunk_id=i) for i in (1, 2)]
        rows += [dict(kind="asr_chunk", chunk_id=i, outcome="emitted", total_seconds=3,
                      queue_seconds=1, processing_seconds=2) for i in (2, 1)]
        rows.append(dict(kind="asr_drained", queued=2, completed=2))
        result = qualification.summarize_events("MEETILY_ISSUE8 " + json.dumps(x) for x in rows)
        self.assertTrue(result["identities_match"])
        self.assertTrue(result["drain_counts_match"])
        self.assertTrue(result["stage_timing_complete"])
        self.assertEqual(result["unexpected_chunk_ids"], [])
        self.assertEqual(result["unfinished_chunk_ids"], [])

    def test_display_and_drain_clocks_require_finite_nonnegative_evidence(self):
        for field, value in (("display_seconds", -1), ("display_seconds", None),
                             ("display_seconds", float("nan")),
                             ("display_seconds", float("inf")),
                             ("drain_completed_seconds", None),
                             ("drain_completed_seconds", float("inf"))):
            with self.subTest(field=field, value=value):
                data = self.journal()
                target = data["runs"][0] if field == "drain_completed_seconds" else data["runs"][0]["landmarks"][0]
                target[field] = value
                self.assertNotEqual(self.review_journal(data)["status"], "manual_review_required")

    def test_api_login_is_not_allowed(self):
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / "codex"
            fake.write_text("#!/bin/sh\necho 'Logged in using an API key'\n")
            fake.chmod(0o755)
            result = qualification.codex(type("Args", (), {"executable": str(fake), "timeout": 5})())
        self.assertEqual(result["status"], "not_run")



if __name__ == "__main__":
    unittest.main()
