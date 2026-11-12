async def __codemode_main():
    global asyncio, fetch, h, hosts, max_cpu, s, series, transient, vals
    import asyncio

    hosts = ["web-01", "web-02", "web-03", "web-04"]

    def transient(e):
        return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

    async def fetch(host):
        for attempt in range(4):  # 1 try + 3 retries
            try:
                return await get_metrics({"host": host, "metric": "cpu"})
            except Exception as e:
                if not transient(e) or attempt == 3:
                    raise
                print(f"{host}: attempt {attempt + 1} failed ({type(e).__name__}: {e}); retrying")
                await asyncio.sleep(0.5 * 2 ** attempt)

    series = await asyncio.gather(*(fetch(h) for h in hosts))

    max_cpu = {}
    for h, s in zip(hosts, series):
        vals = [v for _, v in s["points"] if v is not None]
        max_cpu[h] = max(vals) if vals else None

    print(max_cpu)
    print(await submit_answer({"max_cpu": max_cpu}))
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
