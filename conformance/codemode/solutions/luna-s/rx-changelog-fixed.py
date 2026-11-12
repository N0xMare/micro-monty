import re
text = await read_file({"path": "/docs/CHANGELOG.md"})
issues = set()
lines = text.splitlines()
in_fixed = False
for line in lines:
    if line.startswith("### "):
        in_fixed = line.strip() == "### Fixed"
    elif in_fixed:
        issues.update(int(number) for number in re.findall(r"\(#(\d+)\)", line))
await submit_answer({"issues": sorted(issues)})
