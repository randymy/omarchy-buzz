#!/usr/bin/env python3
"""Run the exact patched authentication flow in disposable PTYs with fake adapters."""
import errno
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import sys
import tempfile
import time


def run(binary, agent, directory, case):
    trace = directory / case
    pid, master = pty.fork()
    if pid == 0:
        env = {'PATH': '/usr/bin:/bin', 'HOME': str(directory), 'LANG': 'C.UTF-8',
               'BUZZ_PRIVATE_KEY': 'synthetic-only', 'OPENAI_API_KEY': 'synthetic-only',
               'ANTHROPIC_API_KEY': 'synthetic-only', 'APP_SERVER_LOGS': 'synthetic-only',
               'CODEX_PATH': 'synthetic-only'}
        os.execve(binary, [binary, str(agent), str(trace), case], env)
    deadline = time.monotonic() + 5
    output = b''
    cancelled = False
    replied = False
    status = None
    try:
        while time.monotonic() < deadline:
            ready, _, _ = select.select([master], [], [], .02)
            if ready:
                try: output += os.read(master, 4096)
                except OSError as error:
                    if error.errno != errno.EIO: raise
            assert len(output) <= 16384
            if case == 'input' and b'synthetic-login-input' in output and not replied:
                os.write(master, b'synthetic-answer\n')
                replied = True
            if case == 'cancel' and Path(str(trace)+'.pid').exists() and not cancelled:
                os.kill(pid, signal.SIGTERM)
                cancelled = True
            finished, status = os.waitpid(pid, os.WNOHANG)
            if finished: break
        else:
            raise AssertionError('fixture deadline: '+case)
        assert os.WIFEXITED(status), (case, status)
        success = case in ('success', 'input', 'descendant', 'agent')
        assert (os.WEXITSTATUS(status) == 0) == success, (case, output)
        events = trace.read_text().splitlines()
        assert 'terminal-restored' in events, (case, events)
        if case == 'agent':
            assert events.count('authenticate') == 1 and 'login' not in events
        else:
            assert 'authenticate' not in events, (case, events)
        if case in ('command', 'unsafe-env', 'duplicate'):
            assert 'login' not in events and events.count('spawn') == 1
        elif case != 'agent':
            assert events.index('shutdown') < events.index('login')
            assert events.count('spawn') == (2 if success or case == 'reconnect-fail' else 1)
        child_pid = Path(str(trace)+'.pid')
        if child_pid.exists():
            descendant = int(child_pid.read_text())
            end = time.monotonic() + 2
            while time.monotonic() < end:
                stat = Path(f'/proc/{descendant}/stat')
                if not stat.exists() or stat.read_text().split(') ',1)[1].startswith('Z '): break
                time.sleep(.02)
            else: raise AssertionError('login descendant still running')
    finally:
        if status is None or not os.WIFEXITED(status):
            try: os.kill(pid, signal.SIGKILL)
            except ProcessLookupError: pass
            try: os.waitpid(pid, 0)
            except ChildProcessError: pass
        os.close(master)


def main():
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    agent = Path(__file__).resolve().parent / 'upstream-acp-terminal/fake-agent.py'
    with tempfile.TemporaryDirectory(prefix='buzz-terminal-fixture-') as temp:
        directory = Path(temp)
        cases = ('success', 'input', 'nonzero', 'timeout', 'cancel', 'descendant',
                 'reconnect-fail', 'command', 'unsafe-env', 'duplicate', 'agent')
        for case in cases: run(binary, agent, directory, case)
        trace = directory / 'no-tty'
        result = subprocess.run([binary, str(agent), str(trace), 'success'],
                                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=5,
                                env={'PATH': '/usr/bin:/bin', 'HOME':temp})
        assert result.returncode != 0
        assert 'login' not in trace.read_text().splitlines()
    print(f'PASS: {len(cases)+1} isolated auth/PTY cases; no provider or real agent invoked')


if __name__ == '__main__': main()
