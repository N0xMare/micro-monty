import asyncio, json

dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
services = json.loads(await list_services({}))
pairs = [(s, d) for d in dates for s in services]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))

counts = {d: 0 for d in dates}
for (s, d), text in zip(pairs, logs):
    for line in text.splitlines():
        parts = line.split()
        if len(parts) > 1 and parts[1] == "ERROR":
            counts[d] += 1

max_count = max(counts.values())
lines = []
for d in dates:
    n = counts[d]
    bar_len = round(n / max_count * 40) if max_count else 0
    lines.append(f"{d} {('#' * bar_len).ljust(40)} {n}")
chart = "\n".join(lines)

print(chart)
print(await post_message({"channel": "#ops", "text": chart}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"text": chart}))
