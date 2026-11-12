import asyncio, json

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
pairs = [(s, d) for s in services for d in dates]
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))
total = 0
for (s, d), text in zip(pairs, texts):
    lines = [l for l in text.splitlines() if l.strip()]
    total += len(lines)
    # Full dumps would flood the output; show counts and a short preview instead.
    print(f"== {s} {d}: {len(lines)} lines")
    for l in lines[:3]:
        print("  ", l)
    if len(lines) > 3:
        print("   ...")
print("total lines:", total)
print(await submit_answer({"lines": total}))
