import asyncio, json
from datetime import datetime, timezone

cpu_raw, mem_raw = await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}),
                                        get_metrics({"host": "web-01", "metric": "mem"}))
cpu = json.loads(cpu_raw)["points"][:8]
mem = json.loads(mem_raw)["points"][:8]
rows = []
for cpu_point, mem_point in zip(cpu, mem):
    time = datetime.fromtimestamp(cpu_point[0], timezone.utc).strftime("%H:%M")
    rows.append({"time": time, "cpu": cpu_point[1], "mem": mem_point[1]})
await submit_answer({"rows": rows})
