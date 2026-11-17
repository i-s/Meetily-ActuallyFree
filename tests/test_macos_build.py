"""Portable build-entrypoint checks; no macOS compilation or capture is exercised."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1] / 'frontend/scripts/build-macos-apple-silicon.sh'


class MacBuild(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.script = self.root / 'frontend/scripts/build-macos-apple-silicon.sh'
        self.script.parent.mkdir(parents=True)
        shutil.copyfile(SOURCE, self.script)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.log = self.root / 'calls'
        self.env = dict(os.environ, PATH=f'{self.bin}:{os.environ["PATH"]}',
                        TEST_ROOT=str(self.root), CALLS=str(self.log),
                        TEST_OS='Darwin', TEST_ARCH='arm64')
        self.stub('uname', 'if [ "$1" = -s ]; then echo "$TEST_OS"; else echo "$TEST_ARCH"; fi')
        self.stub('brew', 'echo "$TEST_ROOT/openssl"')
        self.stub('rustup', 'echo "rustup $*" >> "$CALLS"')
        self.stub('cargo', '''
[ "$OPENSSL_DIR" = "$TEST_ROOT/openssl" ] || exit 9
[ "$OPENSSL_ROOT_DIR" = "$OPENSSL_DIR" ] || exit 9
[ "$MACOSX_DEPLOYMENT_TARGET" = 14.2 ] || exit 9
echo "cargo $*" >> "$CALLS"
mkdir -p "$TEST_ROOT/target/aarch64-apple-darwin/release"
echo sidecar > "$TEST_ROOT/target/aarch64-apple-darwin/release/llama-helper"
''')
        self.stub('pnpm', '''
[ "$MACOSX_DEPLOYMENT_TARGET" = 14.2 ] || exit 9
[ -x "$TEST_ROOT/frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin" ] || exit 9
echo "pnpm $*" >> "$CALLS"
mkdir -p "$TEST_ROOT/target/aarch64-apple-darwin/release/bundle/dmg"
''')
        header = self.root / 'openssl/include/openssl/err.h'
        header.parent.mkdir(parents=True)
        header.touch()

    def stub(self, name, body):
        path = self.bin / name
        path.write_text('#!/bin/bash\nset -eu\n' + body + '\n')
        path.chmod(0o755)

    def run_build(self):
        return subprocess.run(['bash', str(self.script)], env=self.env,
                              capture_output=True, text=True)

    def test_builds_sidecar_before_app_with_locked_dependencies(self):
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.log.read_text().splitlines(), [
            'rustup target add aarch64-apple-darwin',
            'cargo build --locked --release --package llama-helper --target aarch64-apple-darwin --features metal',
            'pnpm exec tauri build --target aarch64-apple-darwin --bundles dmg -- --locked',
        ])

    def test_missing_headers_stops_before_build(self):
        (self.root / 'openssl/include/openssl/err.h').unlink()
        result = self.run_build()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('brew install openssl@3', result.stdout)
        self.assertFalse(self.log.exists())

    def test_rejects_linux_and_rosetta_before_build(self):
        for system, arch in [('Linux', 'aarch64'), ('Darwin', 'x86_64')]:
            with self.subTest(system=system, arch=arch):
                self.env.update(TEST_OS=system, TEST_ARCH=arch)
                self.assertNotEqual(self.run_build().returncode, 0)
                self.assertFalse(self.log.exists())


if __name__ == '__main__':
    unittest.main()
