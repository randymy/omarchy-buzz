"""The panel accepts every capability the helper advertises.

QML fixtures send reduced envelopes, so a helper that gains a capability the
panel's validCapabilities() does not know (or exceeds its length bound) would
pass every QML test and still refuse every real frame.
"""
from pathlib import Path
import json
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


def helper_capabilities():
    source = (ROOT / "helper/src/protocol.rs").read_text()
    match = re.search(r'"capabilities":(\[[^\]]*\])', source)
    return json.loads(match.group(1))


def panel_capabilities():
    source = (ROOT / "plugin/Service.qml").read_text()
    match = re.search(r"readonly property var knownCapabilities: (\[[^\]]*\])", source)
    return json.loads(match.group(1))


class Capabilities(unittest.TestCase):
    def test_panel_knows_every_helper_capability(self):
        helper, panel = helper_capabilities(), panel_capabilities()
        self.assertEqual(len(helper), len(set(helper)), "duplicate helper capability")
        self.assertEqual(sorted(set(helper) - set(panel)), [], "capabilities the panel would refuse")
        self.assertLessEqual(len(helper), len(panel))
        self.assertIn("connection_status", helper)


if __name__ == "__main__":
    unittest.main()
