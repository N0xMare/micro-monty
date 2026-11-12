import re

text = await read_file({"path": "/docs/CHANGELOG.md"})

paren_re = re.compile(r"\(([^()]*#\d+[^()]*)\)")
num_re = re.compile(r"#(\d+)")
heading_re = re.compile(r"^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$")

issues = set()
in_fixed = False
in_code = False
for line in text.splitlines():
    if line.lstrip().startswith("```"):
        in_code = not in_code
        continue
    if in_code:
        continue
    m = heading_re.match(line)
    if m:
        in_fixed = len(m.group(1)) == 3 and m.group(2).strip().lower() == "fixed"
        continue
    if in_fixed:
        for group in paren_re.findall(line):
            issues.update(int(n) for n in num_re.findall(group))

result = sorted(issues)
print(len(result), result)
print(await submit_answer({"issues": result}))
