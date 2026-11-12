import asyncio

services = await list_services({})
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
pairs = [(s, d) for s in services for d in days]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))

total = 0
for (s, d), text in zip(pairs, logs):
    lines = text.splitlines()
    total += len(lines)
    print(f"===== {s} {d} ({len(lines)} lines) =====")
    print("\n".join(lines))
print(f"TOTAL LINES: {total}")
print(await submit_answer({"lines": total}))
