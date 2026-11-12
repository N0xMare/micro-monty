import asyncio, hashlib

sums = await read_file({"path": "/release/SHA256SUMS"})
entries = []
for line in sums.splitlines():
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    h, name = line.split(None, 1)
    name = name.strip()
    if name.startswith("*"):
        name = name[1:]
    entries.append((h.lower(), name))

async def read(name):
    try:
        return await read_file({"path": "/release/" + name})
    except Exception as e:
        print("cannot read", name, e)
        return None

contents = await asyncio.gather(*(read(n) for _, n in entries))
mismatched = sorted(n for (h, n), c in zip(entries, contents)
                    if c is None or hashlib.sha256(c.encode("utf-8")).hexdigest() != h)
print(len(entries), mismatched)
await submit_answer({"mismatched": mismatched, "checked": len(entries)})
