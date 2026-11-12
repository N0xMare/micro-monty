log = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
lines = []
for line in log.splitlines():
    fields = line.split(maxsplit=2)
    if len(fields) >= 2 and fields[1] in ("WARN", "ERROR") and "2026-03-14T12:00:00" <= fields[0] < "2026-03-14T13:00:00":
        lines.append(line)
await submit_answer({"lines": lines})
