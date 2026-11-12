import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
releases = [{"version": m.group(1), "date": m.group(2)} for m in re.finditer(r"^##\s+\[([^\]]+)\]\s+-\s+(\d{4}-\d{2}-\d{2})", text, re.M)]
await submit_answer({"releases": releases})
