import asyncio, json

date = "2026-03-15"
services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": date}) for s in services))

counts = {}
for s, text in zip(services, logs):
    counts[s] = sum(1 for line in text.splitlines()
                    if len(line.split()) > 1 and line.split()[1] == "ERROR")

total = sum(counts.values())
rows = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
lines = [f"Daily error report for {date}", ""]
for s, n in rows:
    share = n / total * 100 if total else 0.0
    lines.append(f"- {s}: {n} errors ({share:.1f}%)")
lines += ["", f"Total: {total}"]
report = "\n".join(lines)

print(report)
print(await post_message({"channel": "#ops", "text": report}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"text": report}))
