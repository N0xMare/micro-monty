import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
fixed = False
issues = set()
for line in text.splitlines():
    heading = re.match(r"^(#{1,3})\s+(.+?)\s*#*\s*$", line)
    if heading:
        fixed = len(heading[1]) == 3 and heading[2].strip() == "Fixed"
    elif fixed:
        issues.update(int(n) for n in re.findall(r"\(#(\d+)\)", line))
await submit_answer({"issues": sorted(issues)})
