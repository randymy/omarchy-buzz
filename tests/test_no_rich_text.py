"""Message text is never read as markup: LinkText is the only rich text in the plugin.

Messages are plain text. To make links clickable, plugin/LinkText.qml renders
text with links as RichText, but only from `Links.render()`, which escapes every
character outside a link and emits nothing but numbered `buzz-link:N` anchors.
Any other use of RichText, StyledText or MarkdownText is a way for relay-supplied
text to become markup, so this fails on each of them outside that one component.
Links are opened only through the helper (`open_link`), never by the panel.
"""
from pathlib import Path
import re
import unittest

PLUGIN = Path(__file__).resolve().parents[1] / "plugin"
ALLOWED = {"LinkText.qml"}
RICH = re.compile(r"\b(RichText|StyledText|MarkdownText)\b")


def code(path):
    """The file without whole-line or trailing `//` comments (a URL has no space before its `//`)."""
    return re.sub(r"(^|\s)//.*$", "", path.read_text(), flags=re.M)


def qml_and_js():
    return sorted(list(PLUGIN.rglob("*.qml")) + list(PLUGIN.rglob("*.js")))


class NoRichText(unittest.TestCase):
    def test_rich_text_only_in_the_link_component(self):
        offenders = []
        for path in qml_and_js():
            if path.name in ALLOWED:
                continue
            for number, line in enumerate(code(path).splitlines(), 1):
                if RICH.search(line):
                    offenders.append(f"{path.name}:{number}: {line.strip()}")
        self.assertEqual(offenders, [], "RichText/StyledText/MarkdownText outside LinkText.qml")

    def test_the_link_component_uses_only_the_escaped_renderer(self):
        text = code(PLUGIN / "LinkText.qml")
        # Rich text is set only from the escaped rendering, never from the source.
        self.assertIn("text = hasLinks ? rendered.html : source", text)
        self.assertIn("textFormat = hasLinks ? TextEdit.RichText : TextEdit.PlainText", text)
        self.assertEqual(text.count("TextEdit.RichText"), 1)
        self.assertNotIn("StyledText", text)
        self.assertNotIn("MarkdownText", text)
        # Nothing but the link renderer produces markup from message text.
        users = [p.name for p in qml_and_js() if "Links.render(" in code(p)]
        self.assertLessEqual(set(users), {"LinkText.qml", "Links.js"})

    def test_only_the_link_renderer_writes_anchors(self):
        writers = [p.name for p in qml_and_js() if re.search(r"<a\b|<img\b|<font\b|<style\b", code(p))]
        self.assertEqual(writers, ["Links.js"])

    def test_links_are_opened_only_through_the_helper(self):
        for path in qml_and_js():
            text = code(path)
            # Two fixed addresses (the Buzz site, the feedback page) are opened that way;
            # nothing that touches message text may be.
            if path.name in {"LinkText.qml", "BuzzMessage.qml", "Links.js", "Service.qml"}:
                self.assertNotIn("openUrlExternally", text, path.name)
            self.assertNotIn("onLinkActivated", text, path.name)

    def test_the_anchor_href_is_an_index_not_the_url(self):
        source = code(PLUGIN / "Links.js")
        self.assertIn('"<a href=\\"buzz-link:" + index + "\\""', source)


if __name__ == "__main__":
    unittest.main()
