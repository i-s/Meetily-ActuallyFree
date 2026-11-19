"""Input/output identity checks; generated files only, not macOS acceptance."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('candidate', ROOT / 'scripts/verify_macos_candidate.py')
candidate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(candidate)


class CandidateChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def source_fixture(self):
        native = self.root / 'frontend/src-tauri'
        native.mkdir(parents=True)
        (native.parent / 'package.json').write_text('{"version":"0.2.18"}')
        self.config = {"version": "0.2.18", "identifier": "com.meetily.ai",
                       "build": {"frontendDist": "../out"},
                       "bundle": {"createUpdaterArtifacts": False}}
        self.write_config()
        (native / 'Cargo.toml').write_text('[package]\nversion="0.2.18"\n')

    def write_config(self):
        (self.root / 'frontend/src-tauri/tauri.conf.json').write_text(json.dumps(self.config))

    def export_fixture(self):
        chunks = self.root / '_next/static/chunks'
        chunks.mkdir(parents=True)
        (chunks / 'app.js').write_text('void 0;')
        for route in candidate.ROUTES:
            (self.root / f'{route}.html').write_text('<script src="/_next/static/chunks/app.js?v=1"></script>')

    def test_matching_source(self):
        self.source_fixture()
        self.assertEqual(candidate.source(self.root)['version'], '0.2.18')

    def test_version_disagreement_is_rejected(self):
        self.source_fixture()
        self.config['version'] = '0.2.19'
        self.write_config()
        with self.assertRaisesRegex(ValueError, 'versions disagree'):
            candidate.source(self.root)

    def test_data_identity_output_path_and_updater_changes_are_rejected(self):
        self.source_fixture()
        for section, key, value in [('build', 'frontendDist', '../wrong'),
                                    ('bundle', 'createUpdaterArtifacts', True),
                                    (None, 'identifier', 'com.other.app')]:
            config = self.config[section] if section else self.config
            original = config[key]
            config[key] = value
            self.write_config()
            with self.assertRaises(ValueError):
                candidate.source(self.root)
            config[key] = original

    def test_export_with_actual_chunks(self):
        self.export_fixture()
        self.assertEqual(candidate.exported(self.root)['routes'], 8)

    def test_successful_build_elsewhere_does_not_supply_expected_export(self):
        self.export_fixture()
        with self.assertRaises(FileNotFoundError):
            candidate.exported(self.root / 'frontend/out')

    def test_missing_route_or_chunk_is_rejected(self):
        self.export_fixture()
        path = self.root / 'settings.html'
        path.unlink()
        with self.assertRaises(FileNotFoundError):
            candidate.exported(self.root)
        path.write_text('<script src="/_next/static/chunks/missing.js"></script>')
        with self.assertRaisesRegex(ValueError, 'missing/empty/outside'):
            candidate.exported(self.root)

    def test_empty_chunks_and_pages_are_rejected(self):
        self.export_fixture()
        (self.root / '_next/static/chunks/app.js').write_text('')
        with self.assertRaises(ValueError):
            candidate.exported(self.root)
        (self.root / 'index.html').write_text('')
        with self.assertRaisesRegex(ValueError, 'empty exported route'):
            candidate.exported(self.root)

    def test_path_escape_is_rejected(self):
        self.export_fixture()
        (self.root / 'index.html').write_text('<script src="/_next/../../../outside.js"></script>')
        with self.assertRaises(ValueError):
            candidate.exported(self.root)


if __name__ == '__main__':
    unittest.main()
