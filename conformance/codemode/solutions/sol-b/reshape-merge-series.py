import asyncio, json
from datetime import datetime, timezone

cpu_raw, mem_raw = await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}), get_metrics({"host": "web-01", "metric": "mem"}))
cpu = dict(json.loads(cpu_raw)["points"])
mem = dict(json.loads(mem_raw)["points"])
times = sorted(cpu.keys() | mem.keys())[:8]
rows = [{"time": datetime.fromtimestamp(t, timezone.utc).strftime("%H:%M"), "cpu": cpu.get(t), "mem": mem.get(t)} for t in times]
await submit_answer({"rows": rows})
