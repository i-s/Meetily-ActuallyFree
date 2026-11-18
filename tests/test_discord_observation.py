import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts' / 'discord_observation_compare.py'
spec = importlib.util.spec_from_file_location('discord_compare', SCRIPT)
sys.dont_write_bytecode = True
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class DiscordComparisonTests(unittest.TestCase):
    def run_report(self, events):
        entries = []
        for at, payload in events:
            if isinstance(payload, str):
                entries.append(dict(seq=len(entries), at_ms=at, scan_from_ms=None, truth=payload, sample=None))
            else:
                base = dict(enabled=True, qualified=False, platform_supported=True, recording_active=False,
                            state='unknown', reason='controls_not_confirmed', session_id='synthetic:1', transition='none')
                base.update(payload)
                entries.append(dict(seq=len(entries), at_ms=at, scan_from_ms=at-100, truth=None, sample=base))
        data = dict(schema=1, mode='observation_only', scenario='dm', active=False, truncated=False, entries=entries)
        return data, module.compare(data)

    def test_join_mute_reconnect_permission_and_leave_open_unknown(self):
        _, report = self.run_report([
            (0, 'idle'), (3000, dict(state='unknown')),
            (5000, 'joined_muted'), (9000, dict(state='active', transition='start')),
            (10000, 'muted'), (13000, dict(state='active')),
            (15000, 'reconnecting'), (18000, dict(reason='reconnect_controls_candidate')),
            (20000, 'reconnected'), (23000, dict(state='active')),
            (25000, 'permission_lost'), (28000, dict(reason='accessibility_permission_required')),
            (30000, 'permission_restored'), (33000, dict(state='active')),
            (35000, 'left_app_open'), (95000, dict()),
        ])
        self.assertEqual(report['qualification'], 'not_granted')
        self.assertEqual(report['violations'], [])
        self.assertEqual(report['counts']['unknown_while_joined'], 2)
        edges = report['transitions_vs_manual_truth']
        self.assertEqual(edges[0]['latency_ms'], 4000)
        self.assertEqual(edges[-1]['truth'], 'left_app_open')
        self.assertEqual(edges[-1]['result'], 'missing')
        self.assertIn('settings', report['missing_required_markers'])

    def test_premature_end_false_start_recording_and_gate_violation_are_explicit(self):
        _, report = self.run_report([
            (0, 'idle'), (3000, dict(state='active', transition='start')),
            (5000, 'joined'), (8000, dict(state='inactive', transition='end')),
            (11000, dict(qualified=True, recording_active=True)),
        ])
        self.assertEqual(report['counts']['candidate_false_join'], 1)
        self.assertEqual(report['counts']['candidate_false_inactive'], 1)
        self.assertEqual({v['error'] for v in report['violations']}, {
            'tracker_false_start', 'tracker_premature_end', 'qualification_gate_not_false', 'recording_active_or_missing'})

    def test_disabled_or_unsupported_is_not_a_negative_supported_call(self):
        _, report = self.run_report([
            (0, 'joined'), (3000, dict(enabled=False, state='inactive')),
            (6000, dict(platform_supported=False, state='inactive')),
            (7000, 'left_app_open'), (8000, dict(state='active')),
        ])
        self.assertEqual(report['counts']['disabled_or_unsupported'], 2)
        self.assertEqual(report['counts'].get('candidate_false_inactive', 0), 0)
        self.assertEqual(report['counts']['truth_boundary_or_unmarked'], 1)

    def test_cli_is_deterministic_and_does_not_grant_qualification(self):
        data, report = self.run_report([(0, 'idle'), (3000, dict())])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'synthetic.json'
            path.write_text(json.dumps(data))
            result = subprocess.run([sys.executable, '-I', str(SCRIPT), str(path)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), report)

    def test_last_sample_end_is_measured_and_short_unknown_interval_is_censored(self):
        _, report = self.run_report([
            (0, 'joined'), (4000, dict(state='active', transition='start')),
            (10000, 'client_quit'), (56000, dict(state='inactive', transition='end')),
        ])
        edge = report['transitions_vs_manual_truth'][-1]
        self.assertEqual(edge['result'], 'observed_within_budget')
        self.assertEqual(edge['latency_ms'], 46000)
        _, short = self.run_report([(0, 'joined'), (3000, dict())])
        self.assertEqual(short['transitions_vs_manual_truth'][0]['result'], 'censored_short_interval')

    def test_rejects_malformed_order_and_arbitrary_manual_content(self):
        data, _ = self.run_report([(0, 'idle')])
        data['entries'][0]['truth'] = 'private chat text'
        with self.assertRaises(ValueError): module.compare(data)
        data['entries'][0]['truth'] = 'idle'
        data['entries'][0]['seq'] = 5
        with self.assertRaises(ValueError): module.compare(data)


if __name__ == '__main__':
    unittest.main()
