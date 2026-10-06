"""Preview tests whose repeating Timer drives synthetic input need a re-entrancy guard.

keyClick, mouseClick, mousePress and mouseRelease spin a nested event loop. The
repeating stage Timer fires again inside it, the nested tick reruns the stage,
fails, and Qt.exit(1) destroys the test under the outer handler, which aborts
Quickshell ("destroyed while one of its QML signal handlers is in progress").
The onTriggered body must start with `if (test.busy) return` (or `inStage`),
set the flag, and clear it in a finally block.
"""
from pathlib import Path
import re
import unittest

TESTS = Path(__file__).resolve().parent
INPUT = re.compile(r"\b(keyClick|mouseClick|mousePress|mouseRelease)\s*\(")
GUARD = re.compile(r"if\s*\([^)\n]*\b(busy|inStage)\b[^)\n]*\)\s*return")


def matching_brace(text, start):
    """Index just past the brace that closes the one opened before `start`."""
    depth, i = 1, start
    while depth and i < len(text):
        depth += {"{": 1, "}": -1}.get(text[i], 0)
        i += 1
    return i


def timer_handlers(text):
    """Yield (line, body) of each onTriggered in a Timer with repeat: true."""
    for timer in re.finditer(r"\bTimer\s*\{", text):
        block = text[timer.end():matching_brace(text, timer.end())]
        if not re.search(r"^\s*repeat:\s*true\b", block, re.M):
            continue
        handler = re.search(r"onTriggered:\s*\{", block)
        if not handler:
            continue
        end = matching_brace(block, handler.end())
        line = text[:timer.end() + handler.start()].count("\n") + 1
        yield line, block[handler.end():end - 1]


def lint(text):
    """Return the problems found in one QML test file's text."""
    if not INPUT.search(text):
        return []
    problems = []
    for line, body in timer_handlers(text):
        code = [l for l in body.splitlines() if l.strip() and not l.strip().startswith("//")]
        # The guard has to come before the stage logic.
        if not GUARD.search("\n".join(code[:4])):
            problems.append("line %d: repeating Timer onTriggered does not start with a busy/inStage return" % line)
        elif "finally" not in "\n".join(code):
            problems.append("line %d: guard flag is not cleared in a finally block" % line)
    return problems


class PreviewTimerGuard(unittest.TestCase):
    def test_timers_driving_synthetic_input_are_guarded(self):
        failures = []
        for path in sorted(TESTS.glob("*.qml")):
            failures += ["%s %s" % (path.name, p) for p in lint(path.read_text())]
        self.assertEqual(failures, [], "\n".join(failures))

    def test_lint_flags_an_unguarded_timer(self):
        bad = ("Item {\n  function k() { input.keyClick(1) }\n  Timer {\n    repeat: true\n"
               "    onTriggered: {\n      try { k() } catch (e) { Qt.exit(1) }\n    }\n  }\n}\n")
        good = bad.replace("try {", "if (test.busy) return\n      test.busy = true\n      try {") \
                  .replace("Qt.exit(1) }", "Qt.exit(1) } finally { test.busy = false }")
        self.assertEqual(len(lint(bad)), 1)
        self.assertEqual(lint(good), [])


if __name__ == "__main__":
    unittest.main()
