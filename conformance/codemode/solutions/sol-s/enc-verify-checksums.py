import asyncio
import hashlib

manifest = await read_file({"path": "/release/SHA256SUMS"})
entries = [line.split("  ", 1) for line in manifest.splitlines() if line.strip()]
contents = await asyncio.gather(*(read_file({"path": "/release/" + name}) for expected, name in entries))
mismatched = [name for (expected, name), content in zip(entries, contents) if hashlib.sha256(content.encode("utf-8")).hexdigest() != expected.lower()]
await submit_answer({"mismatched": sorted(mismatched), "checked": len(entries)})
