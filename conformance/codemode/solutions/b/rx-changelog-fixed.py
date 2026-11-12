import re

text = await read_file({"path": "/docs/CHANGELOG.md"})

issues = set()
in_fixed = False
for line in text.splitlines():
    stripped = line.strip()
    m = re.match(r"^(#{1,6})\s+(.*)$", stripped)
    if m:
        level = len(m.group(1))
        if level == 3:
            in_fixed = m.group(2).strip().lower() == "fixed"
        elif level < 3:
            in_fixed = False
        continue
    if in_fixed:
        issues.update(int(n) for n in re.findall(r"\(#(\d+)\)", line))

result = sorted(issues)
print(result)
print(await submit_answer({"issues": result}))
