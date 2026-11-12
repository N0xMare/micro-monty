import re

text = await read_file({"path": "/docs/CHANGELOG.md"})

releases = []
pattern = re.compile(
    r"^##[ \t]+\[?v?([0-9][^\]\s]*)\]?[ \t]*[-\u2013\u2014][ \t]*(\d{4}-\d{2}-\d{2})",
    re.MULTILINE,
)
for m in pattern.finditer(text):
    releases.append({"version": m.group(1), "date": m.group(2)})

print(releases)
print(await submit_answer({"releases": releases}))
