async def __codemode_main():
    global a, asyncio, deepest, ids, low, lows, results, running, t, txs
    import asyncio

    ids = [f"A{i:03d}" for i in range(1, 13)]
    results = await asyncio.gather(
        *[get_transactions({"account_id": a}) for a in ids], return_exceptions=True
    )

    lows = {}
    for a, txs in zip(ids, results):
        if isinstance(txs, Exception):
            print(a, "error:", txs)
            continue
        running = 0.0
        low = 0.0
        for t in txs:
            running += t["amount"]
            low = min(low, running)
        lows[a] = low
        print(a, len(txs), round(low, 2))

    deepest = min(lows, key=lows.get)
    await submit_answer({"account_id": deepest, "low": round(lows[deepest], 2)})
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
