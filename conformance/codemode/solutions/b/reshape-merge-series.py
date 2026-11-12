import asyncio
import json
from datetime import datetime, timezone, timedelta

cpu_raw, mem_raw = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "mem"}),
)
cpu = {int(t): v for t, v in json.loads(cpu_raw)["points"]}
mem = {int(t): v for t, v in json.loads(mem_raw)["points"]}

base = datetime(2026, 3, 15, tzinfo=timezone.utc)
rows = []
for i in range(8):
    t = base + timedelta(minutes=15 * i)
    ts = int(t.timestamp())
    rows.append({"time": t.strftime("%H:%M"), "cpu": cpu.get(ts), "mem": mem.get(ts)})

print(rows)
print(await submit_answer({"rows": rows}))
