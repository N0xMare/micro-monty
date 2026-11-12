import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
issues = set()
in_fixed = False
fence = None
for line in text.splitlines():
    marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
    if marker:
        token = marker.group(1)
        if fence is None:
            fence = token
        elif token[0] == fence[0] and len(token) >= len(fence):
            fence = None
        continue
    if fence is not None:
        continue
    heading = re.match(r"^ {0,3}(#{1,6})\s+(.+?)\s*#*\s*$", line)
    if heading and len(heading.group(1)) <= 3:
        in_fixed = len(heading.group(1)) == 3 and heading.group(2) == "Fixed"
    elif in_fixed:
        issues.update(int(n) for n in re.findall(r"\(#(\d+)\)", line))
await submit_answer({"issues": sorted(issues)})
