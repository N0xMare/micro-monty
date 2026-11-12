import asyncio

dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
services = await list_services({})

pairs = [(s, d) for s in services for d in dates]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))

counts = {d: 0 for d in dates}
for (s, d), text in zip(pairs, logs):
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "ERROR":
            counts[d] += 1

max_count = max(counts.values())
lines = []
for d in dates:
    c = counts[d]
    bar_len = round(c / max_count * 40) if max_count else 0
    lines.append(f"{d} {('#' * bar_len).ljust(40)} {c}")
chart = "\n".join(lines)
print(chart)

res = await post_message({"channel": "#ops", "text": "```\n" + chart + "\n```"})
print(res)
