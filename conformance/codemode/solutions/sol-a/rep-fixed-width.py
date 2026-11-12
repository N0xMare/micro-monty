import asyncio
import json

hosts = sorted(json.loads(await list_hosts({})), key=lambda h: h["host"])
series = [json.loads(r) for r in await asyncio.gather(*(get_metrics({"host": h["host"], "metric": "cpu"}) for h in hosts))]
lines = ["HOST       MEAN    MAX"]
for host, data in zip(hosts, series):
    values = [v for _, v in data["points"] if v is not None]
    lines.append(f"{host['host']:<8} {sum(values) / len(values):6.1f} {max(values):6.1f}")
await write_file({"path": "/out/cpu.txt", "content": "\n".join(lines) + "\n"})
