import asyncio
import json
from datetime import datetime, timezone

cpu_result, mem_result = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "mem"}),
)
cpu = json.loads(cpu_result)["points"]
mem = {timestamp: value for timestamp, value in json.loads(mem_result)["points"]}
rows = []
for timestamp, cpu_value in cpu[:8]:
    time = datetime.fromtimestamp(timestamp, tz=timezone.utc).strftime("%H:%M")
    rows.append({"time": time, "cpu": cpu_value, "mem": mem.get(timestamp)})
await submit_answer({"rows": rows})
