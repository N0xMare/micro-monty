import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
issues = set()
in_fixed = False
in_fence = False
fence_char = None
fence_length = 0
for line in text.splitlines():
    fence = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
    if fence:
        marker = fence.group(1)
        if not in_fence:
            in_fence = True
            fence_char, fence_length = marker[0], len(marker)
        elif marker[0] == fence_char and len(marker) >= fence_length:
            in_fence = False
        continue
    if in_fence:
        continue
    heading = re.match(r"^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$", line)
    if heading and len(heading.group(1)) <= 3:
        in_fixed = len(heading.group(1)) == 3 and heading.group(2) == "Fixed"
        continue
    if in_fixed:
        issues.update(int(n) for n in re.findall(r"\(#(\d+)\)", line))
await submit_answer({"issues": sorted(issues)})
