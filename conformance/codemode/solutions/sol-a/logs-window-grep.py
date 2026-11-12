from datetime import datetime

log = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
start = datetime.fromisoformat("2026-03-14T12:00:00+00:00")
end = datetime.fromisoformat("2026-03-14T13:00:00+00:00")
lines = []
for line in log.splitlines():
    parts = line.split()
    if len(parts) >= 2 and parts[1] in {"WARN", "ERROR"} and start <= datetime.fromisoformat(parts[0]) < end:
        lines.append(line)
await submit_answer({"lines": lines})
