"""Offline integrity checks for the XBRL agent skill and catalog."""
from pathlib import Path
import re

root = Path(__file__).resolve().parents[1]
skill = (root / "SKILL.md").read_text(encoding="utf-8")
catalog = (root / "references/standards-catalog.md").read_text(encoding="utf-8")
assert skill.startswith("---\n")
assert "name: xbrl-agent" in skill
assert "description: Work with XBRL reports" in skill
for relative in re.findall(r"\]\((references/[^)]+\.md)\)", skill):
    assert (root / relative).is_file(), relative
for reference in (root / "references").glob("*.md"):
    for relative in re.findall(r"\]\(([^)/]+\.md)\)", reference.read_text(encoding="utf-8")):
        assert (reference.parent / relative).is_file(), f"{reference.name}: {relative}"
rows = [
    line for line in catalog.splitlines()
    if line.startswith("| ")
    and (" | Recommendation |" in line or " | Conformance suite |" in line)
]
# Snapshot retrieved 2026-10-26; deliberately update with the catalog when the index changes.
assert len(rows) == 82, len(rows)
urls = re.findall(r"\]\((https?://[^)]+)\)", "\n".join(rows))
assert len(urls) == len(rows)
assert len(set(urls)) == len(urls)
assert all(u.startswith(("https://www.xbrl.org/", "https://xbrl.org/")) for u in urls)
print(f"PASS: local references resolve; {len(rows)} snapshot URLs are unique and first-party (not live-checked)")
