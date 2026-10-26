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
rows = [line for line in catalog.splitlines() if line.startswith("| ") and " | Recommendation |" in line or line.startswith("| ") and " | Conformance suite |" in line]
assert len(rows) == 82, len(rows)
urls = re.findall(r"\]\((https?://[^)]+)\)", "\n".join(rows))
assert len(urls) == len(rows)
assert len(set(urls)) == len(urls)
assert all(u.startswith(("https://www.xbrl.org/", "https://xbrl.org/")) for u in urls)
print(f"PASS: skill references resolve; {len(rows)} official document/suite links are unique")
