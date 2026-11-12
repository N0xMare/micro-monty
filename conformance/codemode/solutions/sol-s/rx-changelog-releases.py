import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
releases = [{"version": v, "date": d} for v, d in re.findall(r"^##\s+\[([^\]]+)\]\s*-\s*(\d{4}-\d{2}-\d{2})", text, re.M)]
await submit_answer({"releases": releases})
