import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
releases = []
for m in re.finditer(r"^##\s*\[([^\]]+)\]\s*[-–—]\s*(\d{4}-\d{2}-\d{2})", text, re.M):
    releases.append({"version": m.group(1).strip(), "date": m.group(2)})
print(releases)
print(await submit_answer({"releases": releases}))
