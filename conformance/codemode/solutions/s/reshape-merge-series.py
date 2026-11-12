import asyncio
from datetime import datetime, timezone, timedelta

cpu, mem = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "mem"}),
)

cpu_map = {int(t): v for t, v in cpu["points"]}
mem_map = {int(t): v for t, v in mem["points"]}

start = datetime(2026, 3, 15, 0, 0, tzinfo=timezone.utc)
rows = []
for i in range(8):
    ts = start + timedelta(minutes=15 * i)
    epoch = int(ts.timestamp())
    rows.append({
        "time": ts.strftime("%H:%M"),
        "cpu": cpu_map.get(epoch),
        "mem": mem_map.get(epoch),
    })

print(rows)
print(await submit_answer({"rows": rows}))
