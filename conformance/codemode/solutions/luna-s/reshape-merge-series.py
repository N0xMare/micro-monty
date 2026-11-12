import asyncio
from datetime import datetime, timezone
cpu, mem = await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}), get_metrics({"host": "web-01", "metric": "mem"}))
cpu_values = dict(cpu["points"])
mem_values = dict(mem["points"])
times = sorted(set(cpu_values) | set(mem_values))[:8]
rows = [{"time": datetime.fromtimestamp(t, timezone.utc).strftime("%H:%M"), "cpu": cpu_values.get(t), "mem": mem_values.get(t)} for t in times]
await submit_answer({"rows": rows})
