"""scripts/pick-file parses name filters and accepts only local file URLs. No portal, bus or dialog."""
import importlib.machinery
import importlib.util
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
loader = importlib.machinery.SourceFileLoader("pick_file", str(ROOT / "scripts" / "pick-file"))
spec = importlib.util.spec_from_loader("pick_file", loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)

assert module.parse_filter("All files (*)") == ("All files", ["*"])
assert module.parse_filter("Images (*.png *.jpg)") == ("Images", ["*.png", "*.jpg"])
assert module.parse_filter("(*.ans)") == ("*.ans", ["*.ans"])
assert module.parse_filter("no parens") is None
assert module.parse_filter("Paths (/etc/* )") is None
assert module.local_path("file:///home/me/a%20b.png") == "/home/me/a b.png"
assert module.local_path("file://localhost/home/me/x") == "/home/me/x"
assert module.local_path("file:///x/../y") == ""
assert module.local_path("file://host/x") == ""
assert module.local_path("https://example.com/x") == ""
assert module.local_path("file:///x/%0Ay") == ""
print("PASS: pick-file parses name filters and keeps only local file URLs")
