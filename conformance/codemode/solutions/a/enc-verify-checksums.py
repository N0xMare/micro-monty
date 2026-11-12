import asyncio
import hashlib

sums = await read_file({"path": "/release/SHA256SUMS"})
entries = []
for line in sums.splitlines():
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    digest, name = line.split(None, 1)
    name = name.strip().lstrip("*")
    entries.append((digest.lower(), name))

async def check(digest, name):
    try:
        content = await read_file({"path": "/release/" + name})
    except Exception as e:
        print("missing", name, e)
        return name, False
    return name, hashlib.sha256(content.encode("utf-8")).hexdigest() == digest

results = await asyncio.gather(*(check(d, n) for d, n in entries))
mismatched = sorted(n for n, ok in results if not ok)
print(len(results), mismatched)
print(await submit_answer({"mismatched": mismatched, "checked": len(results)}))
