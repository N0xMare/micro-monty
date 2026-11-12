text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
lines = []
for line in text.splitlines():
    parts = line.split()
    if len(parts) < 2 or parts[1] not in ("WARN", "ERROR"):
        continue
    t = parts[0][11:19]
    if "12:00:00" <= t < "13:00:00":
        lines.append(line)
print(len(lines), "lines")
print("\n".join(lines))
print(await submit_answer({"lines": lines}))
