#!/usr/bin/python3
"""Synthetic interactive process. No provider, network, login store or secrets."""
import json, os, sys, time, termios
trace, case, *extra = sys.argv[1:]
assert extra == ['--login']
assert os.environ.get('ACP_INTERACTIVE_LOGIN') == '1'
for name in ('BUZZ_PRIVATE_KEY', 'OPENAI_API_KEY', 'ANTHROPIC_API_KEY', 'APP_SERVER_LOGS', 'CODEX_PATH'):
    assert name not in os.environ, 'unexpected inherited variable'
assert all(os.isatty(fd) for fd in range(3))
assert os.tcgetpgrp(0) == os.getpgrp(), 'login process not foreground'
with open(trace, 'a') as out:
    out.write('login\n')
    out.flush()
if case in ('timeout', 'cancel', 'descendant'):
    settings = termios.tcgetattr(0)
    settings[3] &= ~termios.ECHO
    termios.tcsetattr(0, termios.TCSANOW, settings)
    child = os.fork()
    if child == 0:
        while True: time.sleep(1)
    with open(trace + '.pid', 'w') as out: out.write(str(child))
    if case == 'descendant': sys.exit(0)
    while True: time.sleep(1)
if case == 'nonzero': sys.exit(7)
if case == 'input':
    print('synthetic-login-input', flush=True)
    assert input() == 'synthetic-answer'
