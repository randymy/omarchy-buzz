"""Every Controls.ToolTip.text in the plugin goes through PlainText.tip().

Qt tooltips render their text as AutoText and have no textFormat, so relay
text with markup (an <img> in a status or name) would fetch a remote host on
hover. Escaping at every tooltip, not a chosen few, keeps new ones safe.
"""
from pathlib import Path
import re
import unittest

PLUGIN = Path(__file__).resolve().parents[1] / "plugin"


class PlainTooltips(unittest.TestCase):
    def test_every_tooltip_text_is_escaped(self):
        unescaped = []
        for path in sorted(PLUGIN.glob("*.qml")):
            lines = path.read_text().splitlines()
            for number, line in enumerate(lines, 1):
                match = re.match(r"\s*Controls\.ToolTip\.text:\s*(.*)$", line)
                if not match:
                    continue
                expression = match.group(1)
                if expression.rstrip().endswith("{"):
                    # A block: its return value must be wrapped.
                    block = []
                    for following in lines[number:]:
                        block.append(following)
                        if following.strip() == "}":
                            break
                    returns = [b for b in block if b.strip().startswith("return ")]
                    if not returns or not all("PlainText.tip(" in r for r in returns):
                        unescaped.append(f"{path.name}:{number}")
                elif not expression.startswith("PlainText.tip("):
                    unescaped.append(f"{path.name}:{number}")
        self.assertEqual(unescaped, [], "tooltips without PlainText.tip()")

    def test_files_with_tooltips_import_the_helper(self):
        for path in sorted(PLUGIN.glob("*.qml")):
            text = path.read_text()
            if "Controls.ToolTip.text" in text:
                self.assertIn('import "PlainText.js" as PlainText', text, path.name)


if __name__ == "__main__":
    unittest.main()
