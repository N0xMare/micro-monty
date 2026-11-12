import asyncio
import json
from datetime import datetime, timezone

cpu, mem = [json.loads(r) for r in await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}), get_metrics({"host": "web-01", "metric": "mem"}))]
cpu = dict(cpu["points"])
mem = dict(mem["points"])
times = sorted(cpu.keys() | mem.keys())[:8]
rows = [{"time": datetime.fromtimestamp(t, timezone.utc).strftime("%H:%M"), "cpu": cpu.get(t), "mem": mem.get(t)} for t in times]
await submit_answer({"rows": rows})
