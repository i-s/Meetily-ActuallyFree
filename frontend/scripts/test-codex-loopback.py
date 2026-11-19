#!/usr/bin/env python3
"""Run an already-compiled Codex contract test binary on Linux, loopback only.

Usage: python3 -I frontend/scripts/test-codex-loopback.py \
    /absolute/portable-test-executable /absolute/codex 'codex-cli 0.170.0'
Compile first with test-codex-portable.py. No accounts or credentials are used.
Requires working unprivileged user/network namespaces, bubblewrap, iproute2 and
setpriv. Test executable and CLI must be outside the masked real-home mounts.
"""
from pathlib import Path
import os
import shutil
import signal
import subprocess
import sys
import tempfile


def main():
    if sys.platform != 'linux' or len(sys.argv) != 4:
        raise SystemExit(__doc__)
    tests, binary = (Path(value) for value in sys.argv[1:3])
    for path in (tests, binary):
        if not path.is_absolute() or not path.is_file() or not os.access(path, os.X_OK):
            raise SystemExit(f'An absolute executable path is required: {path}')
    bwrap, ip, setpriv = (shutil.which(name) for name in ('bwrap', 'ip', 'setpriv'))
    if not bwrap or not ip or not setpriv:
        raise SystemExit('bubblewrap, iproute2 and setpriv are required; no unsandboxed fallback')
    with tempfile.TemporaryDirectory(prefix='meetily-codex-loopback-') as home:
        # The private namespace has no external interface or route. Start only
        # loopback so the synthetic Responses listener works. Hide real homes,
        # clear credentials/proxy/provider/preload/session env, and retain no
        # capabilities in the Rust test runner or its CLI descendants.
        guard = '''import os, sys
from pathlib import Path
interfaces = {line.split(':')[0].strip() for line in Path('/proc/net/dev').read_text().splitlines()[2:]}
assert interfaces == {'lo'}, interfaces
assert len(Path('/proc/net/route').read_text().splitlines()) == 1, 'external route present'
for line in Path('/proc/self/status').read_text().splitlines():
    if line.startswith(('CapInh:', 'CapPrm:', 'CapEff:', 'CapBnd:', 'CapAmb:')):
        assert int(line.split()[1], 16) == 0, line
assert not list(Path('/var/lib/t3-code').iterdir()), 'real home is visible'
os.execv(sys.argv[1], sys.argv[1:])
'''
        command = [
            bwrap, '--unshare-user', '--uid', '0', '--gid', '0',
            '--unshare-net', '--unshare-pid', '--die-with-parent',
            '--cap-add', 'CAP_NET_ADMIN', '--cap-add', 'CAP_SETPCAP',
            '--ro-bind', '/', '/',
            '--bind', '/tmp', '/tmp', '--tmpfs', '/var/lib/t3-code',
            '--tmpfs', '/root', '--proc', '/proc', '--dev', '/dev',
            '--chdir', '/tmp', '--clearenv', '--setenv', 'HOME', home,
            '--setenv', 'USERPROFILE', home, '--setenv', 'PATH', '/usr/bin:/bin',
            '--setenv', 'MEETILY_CODEX_TEST_BINARY', str(binary),
            '--setenv', 'MEETILY_CODEX_EXPECT_VERSION', sys.argv[3],
            '--', '/bin/sh', '-c', '"$1" link set lo up && shift && exec "$@"',
            'loopback', ip,
            setpriv, '--bounding-set=-all', '--inh-caps=-all', '--ambient-caps=-all',
            str(Path(sys.executable).resolve()), '-I', '-c', guard,
            str(tests), 'real_binary', '--ignored', '--test-threads=1',
        ]
        process = subprocess.Popen(command, start_new_session=True)
        try:
            code = process.wait(timeout=180)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise
        raise SystemExit(code)


if __name__ == '__main__':
    main()
