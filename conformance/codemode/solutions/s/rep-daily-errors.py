import asyncio

date = "2026-03-15"
services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": date}) for s in services))

counts = {}
for s, text in zip(services, logs):
    n = 0
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "ERROR":
            n += 1
    counts[s] = n

total = sum(counts.values())
rows = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
lines = [f"Daily error report for {date}", ""]
for s, n in rows:
    share = (n / total * 100) if total else 0.0
    lines.append(f"- {s}: {n} errors ({share:.1f}%)")
lines += ["", f"Total: {total}"]
text = "\n".join(lines)
print(text)

res = await post_message({"channel": "#ops", "text": text})
print(res)
