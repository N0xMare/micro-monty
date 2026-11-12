import re

text = await read_file({"path": "/docs/CHANGELOG.md"})

issues = set()
in_fixed = False
in_code = False
for line in text.splitlines():
    s = line.strip()
    if s.startswith("```") or s.startswith("~~~"):
        in_code = not in_code
        continue
    if in_code:
        continue
    m = re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", s)
    if m:
        level = len(m.group(1))
        if level <= 3:
            in_fixed = level == 3 and m.group(2).strip().lower() == "fixed"
        continue
    if in_fixed:
        for group in re.findall(r"\(([^()]*#\d+[^()]*)\)", line):
            issues.update(int(n) for n in re.findall(r"#(\d+)", group))

issues = sorted(issues)
print(issues)
print(await submit_answer({"issues": issues}))
