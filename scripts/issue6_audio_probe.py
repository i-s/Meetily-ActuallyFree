#!/usr/bin/env python3
"""Local, read-only issue #6 evidence probe; never changes audio or labels."""
import argparse
from array import array
import hashlib
import json
import math
from pathlib import Path
import sys
import unicodedata
import wave


MAX_DURATION = 10.0


def finite_number(value, name):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f'{name} must be a finite number')
    return float(value)


def read_window(path, start, duration):
    """Read bounded PCM, not an entire 90-minute recording. Time is in seconds."""
    with wave.open(str(path), 'rb') as wav:
        if wav.getnchannels() != 1 or wav.getsampwidth() != 2 or wav.getcomptype() != 'NONE':
            raise ValueError(f'{path}: require mono signed PCM16 WAV (decode explicitly first)')
        rate = wav.getframerate()
        if not 1000 <= rate <= 192000:
            raise ValueError(f'{path}: require sample rate 1000..192000 Hz')
        if not math.isfinite(start) or start < 0 or start + duration > wav.getnframes() / rate:
            raise ValueError(f'{path}: requested window is outside the track')
        first = round(start * rate)
        count = round(duration * rate)
        if first < 0 or count < 1 or first + count > wav.getnframes():
            raise ValueError(f'{path}: requested window is outside the track')
        wav.setpos(first)
        raw = wav.readframes(count)
        if len(raw) != count * 2:
            raise ValueError(f'{path}: truncated PCM data')
    samples = array('h')
    samples.frombytes(raw)
    if sys.byteorder != 'little':
        samples.byteswap()
    return rate, samples


def levels(samples):
    rms = math.sqrt(sum((x / 32768.0) ** 2 for x in samples) / len(samples))
    return {
        'rms_dbfs': 20 * math.log10(rms) if rms else None,
        'peak_dbfs': 20 * math.log10(max(abs(x) for x in samples) / 32768.0)
        if any(samples) else None,
        'clipped_samples': sum(x in (-32768, 32767) for x in samples),
        'samples': len(samples),
    }


def correlation(reference, observed):
    a = sum(reference) / len(reference)
    b = sum(observed) / len(observed)
    xx = yy = xy = 0.0
    for x, y in zip(reference, observed):
        x, y = x - a, y - b
        xx += x * x
        yy += y * y
        xy += x * y
    return xy / math.sqrt(xx * yy) if xx and yy else None


def track_probe(mic, system, start, duration, mic_offset, max_lag_ms):
    start = finite_number(start, 'start')
    duration = finite_number(duration, 'duration')
    mic_offset = finite_number(mic_offset, 'mic offset')
    max_lag_ms = finite_number(max_lag_ms, 'max lag')
    if start < 0 or not 0.1 <= duration <= MAX_DURATION or not 0 <= max_lag_ms <= 500:
        raise ValueError('start >= 0, duration 0.1..10 seconds, max lag 0..500 ms required')
    # Positive offset means the mic file's corresponding sample is later than
    # the system file's. Do not silently trim or align away capture drift.
    sr, reference = read_window(system, start, duration)
    mr, observed = read_window(mic, start + mic_offset, duration)
    if sr != mr:
        raise ValueError('track sample rates differ; decode both to the same rate explicitly')
    # Block averages bound scan cost independently of capture rate. This coarse
    # waveform similarity is not an AEC decision or a sample-accurate clock test.
    block = max(1, math.ceil(sr / 1000))
    def reduce(samples):
        return [sum(samples[i:i + block]) / block
                for i in range(0, len(samples) - block + 1, block)]
    ref, obs = reduce(reference), reduce(observed)
    rate = sr / block
    limit = min(round(max_lag_ms * rate / 1000), len(ref) // 4)
    step = max(1, round(rate * 0.005))
    candidates = []
    for lag in sorted(set(range(-limit, limit + 1, step)) | {0, -limit, limit}):
        lo = max(0, -lag)
        hi = min(len(ref), len(obs) - lag)
        value = correlation(ref[lo:hi], obs[lo + lag:hi + lag])
        if value is not None:
            candidates.append((abs(value), value, lag))
    best = max(candidates) if candidates else None
    return {
        'kind': 'track_observations_not_acceptance',
        'system_start_seconds': start,
        'mic_start_seconds': start + mic_offset,
        'duration_seconds': duration,
        'sample_rate_hz': sr,
        'mic': levels(observed),
        'system': levels(reference),
        'coarse_correlation': None if best is None else {
            'coefficient': best[1],
            'mic_delay_relative_to_system_ms': best[2] * 1000 / rate,
            'resolution_ms': step * 1000 / rate,
            'searched_max_lag_ms': limit * 1000 / rate,
        },
        'limits': 'Similarity is not causation, separation quality, speech retention, or AEC proof. '
                  'Silence returns null dBFS/correlation. Inspect source audio and route controls.',
    }


def tokens(text):
    text = unicodedata.normalize('NFKC', text).casefold()
    return ''.join(c if c.isalnum() else ' ' for c in text).split()


def contains_phrase(text, phrase):
    words, expected = tokens(text), tokens(phrase)
    return bool(expected) and any(words[i:i + len(expected)] == expected
                                  for i in range(len(words) - len(expected) + 1))


def validate_interval(item, name):
    start = finite_number(item.get('start'), f'{name}.start')
    end = finite_number(item.get('end'), f'{name}.end')
    if start < 0 or end <= start:
        raise ValueError(f'{name}: require 0 <= start < end')
    return start, end


def transcript_probe(data):
    """Check an explicit local adapter format, never infer source from a label."""
    if not isinstance(data, dict) or data.get('schema_version') != 1:
        raise ValueError('require schema_version 1')
    rows, cases = data.get('transcript'), data.get('cases')
    if not isinstance(rows, list) or not isinstance(cases, list) or not cases:
        raise ValueError('require transcript array and nonempty cases array')
    labels = data.get('you_labels')
    if not isinstance(labels, list) or not labels or any(not isinstance(x, str) or not x.strip() for x in labels):
        raise ValueError('you_labels must explicitly include the actual local-user labels')
    for i, row in enumerate(rows):
        if not isinstance(row, dict):
            raise ValueError(f'transcript[{i}] must be an object')
        validate_interval(row, f'transcript[{i}]')
        if row.get('capture_source') not in ('microphone', 'system', 'unknown'):
            raise ValueError('capture_source must be microphone/system/unknown, not inferred from speaker')
        if not isinstance(row.get('text'), str) or not isinstance(row.get('label'), str):
            raise ValueError('each transcript row requires text and label strings')
    results = []
    ids = set()
    for i, case in enumerate(cases):
        if not isinstance(case, dict):
            raise ValueError(f'cases[{i}] must be an object')
        start, end = validate_interval(case, f'cases[{i}]')
        case_id, mode = case.get('id'), case.get('mode')
        if not isinstance(case_id, str) or not case_id.strip() or case_id in ids:
            raise ValueError('case IDs must be unique nonempty strings')
        ids.add(case_id)
        if mode not in ('remote-only', 'near-only', 'double-talk'):
            raise ValueError('case mode must be remote-only/near-only/double-talk')
        selected = [(j, r) for j, r in enumerate(rows) if r['start'] < end and r['end'] > start]
        # A boundary-spanning row is a review item, not proof that the words were
        # spoken inside this case; ASR generally does not supply word timestamps.
        boundary = [j for j, r in selected if r['start'] < start or r['end'] > end]
        false_you = [j for j, r in selected if mode == 'remote-only'
                     and r['label'] in labels and r['text'].strip()]
        expected = case.get('expected_near')
        if mode != 'remote-only' and (not isinstance(expected, str) or not tokens(expected)):
            raise ValueError('near-only/double-talk require nonempty expected_near test phrase')
        expected_remote = case.get('expected_remote')
        if mode != 'near-only' and (not isinstance(expected_remote, str) or not tokens(expected_remote)):
            raise ValueError('remote-only/double-talk require nonempty expected_remote test phrase')
        remote_matches = [] if mode == 'near-only' else [j for j, r in selected
                         if r['capture_source'] == 'system' and r['label'] not in labels
                         and contains_phrase(r['text'], expected_remote)]
        matches = [] if mode == 'remote-only' else [j for j, r in selected
                   if r['capture_source'] == 'microphone' and r['label'] in labels
                   and contains_phrase(r['text'], expected)]
        results.append({
            'id': case_id, 'mode': mode,
            'overlapping_rows': [j for j, _ in selected],
            'boundary_review_rows': boundary,
            'remote_only_you_rows': false_you,
            'expected_near_phrase_rows': matches,
            'expected_remote_phrase_rows': remote_matches,
            'unknown_source_rows': [j for j, r in selected if r['capture_source'] == 'unknown'],
            'status': 'manual_review_required',
        })
    return {
        'kind': 'transcript_observations_not_acceptance', 'cases': results,
        'limits': 'No rows is not a pass. Phrase absence can be ASR error or speech loss; '
                  'phrase presence does not prove full retention. Review word errors, source audio, '
                  'boundary rows, timestamps, route controls and ducking manually. Labels must '
                  'include display-name substitutions and saved/manual names.',
    }


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    tracks = sub.add_parser('tracks', help='bounded mono PCM16 source-track observations')
    tracks.add_argument('--mic', type=Path, required=True)
    tracks.add_argument('--system', type=Path, required=True)
    tracks.add_argument('--start', type=float, required=True, help='system file seconds')
    tracks.add_argument('--duration', type=float, default=3.0)
    tracks.add_argument('--mic-offset', type=float, default=0.0, help='mic file seconds minus system file seconds')
    tracks.add_argument('--max-lag-ms', type=float, default=250.0)
    transcript = sub.add_parser('transcript', help='check manually adapted transcript evidence JSON')
    transcript.add_argument('evidence', type=Path)
    hashes = sub.add_parser('hash', help='hash original local evidence files without uploading')
    hashes.add_argument('files', type=Path, nargs='+')
    args = parser.parse_args()
    try:
        if args.command == 'tracks':
            result = track_probe(args.mic, args.system, args.start, args.duration,
                                 args.mic_offset, args.max_lag_ms)
        elif args.command == 'transcript':
            result = transcript_probe(json.loads(args.evidence.read_text(encoding='utf-8')))
        else:
            result = {'sha256': {str(path): sha256(path) for path in args.files}}
    except (ValueError, OSError, wave.Error, EOFError) as error:
        parser.exit(2, f'error: {error}\n')
    print(json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
