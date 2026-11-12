import re

text = await read_file({"path": "/docs/CHANGELOG.md"})
releases = [
    {"version": version, "date": date}
    for version, date in re.findall(r"^## \[([^\]]+)\] - (\d{4}-\d{2}-\d{2})\s*$", text, re.M)
]
await submit_answer({"releases": releases})
