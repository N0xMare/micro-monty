import re

text = await read_file({"path": "/docs/CHANGELOG.md"})

pattern = re.compile(
    r'^##\s+\[([^\]]+)\]\s*[-\u2013\u2014]\s*(\d{4}-\d{2}-\d{2})\b',
    re.MULTILINE,
)
releases = [{"version": m.group(1).strip(), "date": m.group(2)} for m in pattern.finditer(text)]

print(len(releases), releases)
await submit_answer({"releases": releases})
