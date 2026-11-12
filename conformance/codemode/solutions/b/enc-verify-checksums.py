import asyncio, hashlib

sums = await read_file({"path": "/release/SHA256SUMS"})
entries = []
for line in sums.splitlines():
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    digest, name = line.split(None, 1)
    entries.append((digest.lower(), name.strip().lstrip("*")))

results = await asyncio.gather(
    *(read_file({"path": f"/release/{name}"}) for _, name in entries),
    return_exceptions=True,
)

mismatched = []
for (digest, name), content in zip(entries, results):
    if isinstance(content, Exception):
        print("unreadable:", name, content)
        mismatched.append(name)
    elif hashlib.sha256(content.encode("utf-8")).hexdigest() != digest:
        mismatched.append(name)

mismatched.sort()
print(len(entries), mismatched)
print(await submit_answer({"mismatched": mismatched, "checked": len(entries)}))
