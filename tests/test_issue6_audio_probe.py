"""Portable issue #6 probe tests. Synthetic PCM is not Mac/AEC/ASR qualification."""
from array import array
import importlib.util
import json
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest
import wave

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/issue6_audio_probe.py'
spec = importlib.util.spec_from_file_location('issue6_audio_probe', SCRIPT)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class AudioProbe(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        rng = random.Random(6)
        self.remote = [rng.randrange(-8000, 8000) for _ in range(3000)]
        self.near = [rng.randrange(-4000, 4000) for _ in range(3000)]

    def wav(self, name, samples, rate=1000, channels=1):
        path = self.root / name
        data = array('h', samples)
        if sys.byteorder != 'little':
            data.byteswap()
        with wave.open(str(path), 'wb') as wav:
            wav.setnchannels(channels)
            wav.setsampwidth(2)
            wav.setframerate(rate)
            wav.writeframes(data.tobytes())
        return path

    def test_delayed_remote_copy_is_observed_not_removed(self):
        system = self.wav('system.wav', self.remote)
        mic = self.wav('mic.wav', [0] * 40 + self.remote[:-40])
        before = probe.sha256(mic)
        result = probe.track_probe(mic, system, 0, 3, 0, 100)
        self.assertAlmostEqual(result['coarse_correlation']['coefficient'], 1)
        self.assertEqual(result['coarse_correlation']['mic_delay_relative_to_system_ms'], 40)
        self.assertEqual(probe.sha256(mic), before)
        self.assertEqual(result['kind'], 'track_observations_not_acceptance')

    def test_double_talk_preserves_both_input_files(self):
        system = self.wav('system.wav', self.remote)
        mic = self.wav('mic.wav', [r // 2 + n for r, n in zip(self.remote, self.near)])
        before = (mic.read_bytes(), system.read_bytes())
        result = probe.track_probe(mic, system, 0, 3, 0, 100)
        self.assertGreater(result['coarse_correlation']['coefficient'], 0.3)
        self.assertLess(result['coarse_correlation']['coefficient'], 0.95)
        self.assertEqual((mic.read_bytes(), system.read_bytes()), before)
        # Similarity during double-talk must not trigger a destructive decision.
        self.assertNotIn('suppress', result)

    def test_silence_is_null_not_perfect_correlation(self):
        silence = self.wav('silence.wav', [0] * 3000)
        result = probe.track_probe(silence, silence, 0, 3, 0, 100)
        self.assertIsNone(result['mic']['rms_dbfs'])
        self.assertIsNone(result['coarse_correlation'])
        json.dumps(result, allow_nan=False)

    def test_explicit_offset_and_clipping(self):
        system = self.wav('system.wav', self.remote)
        mic = self.wav('mic.wav', [0] * 100 + self.remote)
        result = probe.track_probe(mic, system, 0, 3, 0.1, 0)
        self.assertAlmostEqual(result['coarse_correlation']['coefficient'], 1)
        self.assertEqual(probe.levels(array('h', [-32768, 32767, 0]))['clipped_samples'], 2)

    def test_rejects_out_of_bounds_mismatched_rate_stereo_and_nonfinite(self):
        good = self.wav('good.wav', self.remote)
        bad_rate = self.wav('rate.wav', self.remote, rate=2000)
        stereo = self.wav('stereo.wav', self.remote, channels=2)
        for args in [(good, good, -1, 1, 0, 100),
                     (good, good, 0, 11, 0, 100),
                     (good, good, 2, 2, 0, 100),
                     (good, good, 0, 1, -1, 100),
                     (good, good, 0, 1, 0, 501),
                     (good, good, float('nan'), 1, 0, 0),
                     (good, good, 1e308, 1, 0, 0),
                     (good, good, 0, 1, 1e308, 0),
                     (good, bad_rate, 0, 1, 0, 0),
                     (stereo, good, 0, 1, 0, 0)]:
            with self.subTest(args=args), self.assertRaises(ValueError):
                probe.track_probe(*args)

    def test_truncated_wav_is_rejected(self):
        path = self.wav('truncated.wav', self.remote)
        path.write_bytes(path.read_bytes()[:-20])
        with self.assertRaisesRegex(ValueError, 'truncated'):
            probe.track_probe(path, path, 0, 3, 0, 0)

    def test_cli_hashes_original_files(self):
        import hashlib
        path = self.root / 'owned-test.txt'
        path.write_bytes(b'owned synthetic fixture')
        result = subprocess.run([sys.executable, '-I', '-B', str(SCRIPT), 'hash', str(path)],
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['sha256'][str(path)],
                         hashlib.sha256(path.read_bytes()).hexdigest())

    def test_cli_emits_json_and_rejects_invalid_input(self):
        good = self.wav('good.wav', self.remote)
        command = [sys.executable, '-I', str(SCRIPT), 'tracks', '--mic', str(good),
                   '--system', str(good), '--start', '0']
        result = subprocess.run(command, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['sample_rate_hz'], 1000)
        failed = subprocess.run(command + ['--duration', '90'], text=True, capture_output=True)
        self.assertEqual(failed.returncode, 2)
        self.assertNotIn('Traceback', failed.stderr)


class TranscriptProbe(unittest.TestCase):
    def evidence(self):
        return {'schema_version': 1, 'you_labels': ['You', 'Я', 'Local Test'],
                'cases': [{'id': 'remote', 'mode': 'remote-only', 'start': 0, 'end': 10,
                           'expected_remote': 'Remote blue ship'},
                          {'id': 'overlap', 'mode': 'double-talk', 'start': 10, 'end': 20,
                           'expected_near': 'Мой зелёный мост',
                           'expected_remote': 'Simultaneous remote words'}],
                'transcript': [
                    {'start': 2, 'end': 4, 'label': 'You', 'capture_source': 'microphone',
                     'text': 'Remote blue ship'},
                    {'start': 2, 'end': 4, 'label': 'Guest', 'capture_source': 'system',
                     'text': 'Remote blue ship'},
                    {'start': 12, 'end': 14, 'label': 'Я', 'capture_source': 'microphone',
                     'text': 'Мой зелёный мост!'},
                    {'start': 12, 'end': 14, 'label': 'Guest', 'capture_source': 'system',
                     'text': 'Simultaneous remote words'}]}

    def test_false_you_and_near_phrase_in_double_talk(self):
        result = probe.transcript_probe(self.evidence())
        self.assertEqual(result['cases'][0]['remote_only_you_rows'], [0])
        self.assertEqual(result['cases'][1]['expected_near_phrase_rows'], [2])
        self.assertEqual(result['cases'][0]['expected_remote_phrase_rows'], [1])
        self.assertEqual(result['cases'][1]['expected_remote_phrase_rows'], [3])
        self.assertTrue(all(c['status'] == 'manual_review_required' for c in result['cases']))

    def test_empty_transcript_does_not_pass(self):
        data = self.evidence()
        data['transcript'] = []
        result = probe.transcript_probe(data)
        self.assertEqual(result['cases'][1]['expected_near_phrase_rows'], [])
        self.assertEqual(result['cases'][0]['status'], 'manual_review_required')

    def test_unknown_source_not_inferred_from_you(self):
        data = self.evidence()
        data['transcript'][2]['capture_source'] = 'unknown'
        result = probe.transcript_probe(data)['cases'][1]
        self.assertEqual(result['unknown_source_rows'], [2])
        self.assertEqual(result['expected_near_phrase_rows'], [])

    def test_boundary_spanning_rows_require_review(self):
        data = self.evidence()
        data['transcript'][0]['end'] = 11
        result = probe.transcript_probe(data)['cases']
        self.assertEqual(result[0]['boundary_review_rows'], [0])
        self.assertEqual(result[1]['boundary_review_rows'], [0])

    def test_phrase_matching_is_unicode_word_based_not_substring(self):
        self.assertTrue(probe.contains_phrase('MY green bridge.', 'my green bridge'))
        self.assertFalse(probe.contains_phrase('notmy green bridges', 'my green bridge'))

    def test_rejects_missing_labels_source_invalid_time_and_expected_phrase(self):
        for mutate in [lambda d: d.pop('you_labels'),
                       lambda d: d['transcript'][0].pop('capture_source'),
                       lambda d: d['transcript'][0].update(end=float('inf')),
                       lambda d: d['cases'][1].update(expected_near=''),
                       lambda d: d['cases'][1].update(id='remote')]:
            data = self.evidence()
            mutate(data)
            with self.assertRaises(ValueError):
                probe.transcript_probe(data)

    def test_example_is_runnable_and_explicitly_pending(self):
        path = SCRIPT.parent.parent / 'docs/fixtures/issue6-evidence.example.json'
        data = json.loads(path.read_text())
        result = probe.transcript_probe(data)
        self.assertEqual(data['physical_acceptance'], 'not_run')
        self.assertTrue(all(c['status'] == 'manual_review_required' for c in result['cases']))


if __name__ == '__main__':
    unittest.main()
