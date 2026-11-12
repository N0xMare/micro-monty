import asyncio, hashlib

manifest = await read_file({"path": "/release/SHA256SUMS"})
entries = []
for line in manifest.splitlines():
    if line.strip():
        expected, name = line.split(None, 1)
        entries.append((expected, name.lstrip(" *")))
contents = await asyncio.gather(*(read_file({"path": "/release/" + name}) for _, name in entries))
mismatched = sorted(name for (expected, name), content in zip(entries, contents) if hashlib.sha256(content.encode("utf-8")).hexdigest() != expected)
await submit_answer({"mismatched": mismatched, "checked": len(entries)})
