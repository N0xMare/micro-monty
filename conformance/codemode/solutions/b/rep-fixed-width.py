import asyncio, json

hosts = sorted(h["host"] for h in json.loads(await list_hosts({})))
series = [json.loads(s) for s in await asyncio.gather(*(get_metrics({"host": h, "metric": "cpu"}) for h in hosts))]

lines = ["HOST       MEAN    MAX"]
for h, m in zip(hosts, series):
    vals = [v for _, v in m["points"] if v is not None]
    if not vals:
        print(f"warning: {h} has no cpu samples")
        lines.append(f"{h:<8} {'n/a':>6} {'n/a':>6}")
        continue
    lines.append(f"{h:<8} {sum(vals) / len(vals):6.1f} {max(vals):6.1f}")
content = "\n".join(lines) + "\n"

print(content)
print(await write_file({"path": "/out/cpu.txt", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/cpu.txt"}))
