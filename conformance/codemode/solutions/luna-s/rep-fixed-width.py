import asyncio
hosts = await list_hosts({})
data = await asyncio.gather(*(get_metrics({"host": h["host"], "metric": "cpu"}) for h in hosts))
lines = ["HOST       MEAN    MAX"]
for result in sorted(data, key=lambda x: x["host"]):
    values = [v for _, v in result["points"] if v is not None]
    if values:
        lines.append(f"{result['host']:<8} {sum(values) / len(values):>6.1f} {max(values):>6.1f}")
await write_file({"path": "/out/cpu.txt", "content": "\n".join(lines) + "\n"})
