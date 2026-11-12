import asyncio, json

async def fetch(host):
    for attempt in range(4):
        try:
            raw = await get_metrics({"host": host, "metric": "cpu"})
        except (TimeoutError, RuntimeError) as exc:
            if not (isinstance(exc, TimeoutError) or "503" in str(exc)) or attempt == 3:
                raise
        else:
            return json.loads(raw)

hosts = ["web-01", "web-02", "web-03", "web-04"]
series = await asyncio.gather(*(fetch(h) for h in hosts))
await submit_answer({"max_cpu": {h: max(v for t, v in data["points"] if v is not None) for h, data in zip(hosts, series)}})
