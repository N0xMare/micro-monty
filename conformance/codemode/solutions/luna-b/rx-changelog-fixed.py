import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
issues = set()
in_fixed = False
for line in text.splitlines():
    if line.startswith("### "):
        in_fixed = line.strip() == "### Fixed"
    elif line.startswith("#"):
        in_fixed = False
    elif in_fixed:
        issues.update(int(number) for number in re.findall(r"\(#(\d+)\)", line))
await submit_answer({"issues": sorted(issues)})
