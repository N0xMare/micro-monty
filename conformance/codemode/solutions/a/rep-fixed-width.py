import asyncio, json

hosts = sorted(h["host"] for h in json.loads(await list_hosts({})))
series = [json.loads(r) for r in await asyncio.gather(
    *(get_metrics({"host": h, "metric": "cpu"}) for h in hosts))]

lines = ["HOST       MEAN    MAX"]
for h, s in zip(hosts, series):
    vals = [v for _, v in s["points"] if v is not None]
    if not vals:
        continue
    mean = sum(vals) / len(vals)
    lines.append(f"{h:<8} {mean:>6.1f} {max(vals):>6.1f}")
content = "\n".join(lines) + "\n"
print(content)
print(await write_file({"path": "/out/cpu.txt", "content": content}))
