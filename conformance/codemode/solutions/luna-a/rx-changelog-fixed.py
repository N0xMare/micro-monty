import re

content = await read_file({"path": "/docs/CHANGELOG.md"})
lines = content.splitlines()
issues = []
in_fixed = False
for line in lines:
    if line.startswith("### "):
        in_fixed = line.strip() == "### Fixed"
    elif line.startswith("## ") or line.startswith("# "):
        in_fixed = False
    if in_fixed:
        issues.extend(int(number) for number in re.findall(r"\(#(\d+)\)", line))
await submit_answer({"issues": sorted(issues)})
