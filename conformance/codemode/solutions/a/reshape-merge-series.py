import asyncio, json
from datetime import datetime, timezone

cpu_raw, mem_raw = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "mem"}),
)
cpu = {int(t): v for t, v in json.loads(cpu_raw)["points"]}
mem = {int(t): v for t, v in json.loads(mem_raw)["points"]}

start = int(datetime(2026, 3, 15, 0, 0, tzinfo=timezone.utc).timestamp())
rows = []
for i in range(8):
    t = start + i * 900
    rows.append({
        "time": datetime.fromtimestamp(t, timezone.utc).strftime("%H:%M"),
        "cpu": cpu.get(t),
        "mem": mem.get(t),
    })
for r in rows:
    print(r)
print(await submit_answer({"rows": rows}))
