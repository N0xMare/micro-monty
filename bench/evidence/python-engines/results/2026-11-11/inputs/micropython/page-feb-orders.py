async def __codemode_main():
    global Counter, args, counts, cursor, o, resp, seen
    from collections import Counter

    counts = Counter()
    seen = set()
    cursor = None
    while True:
        args = {"since": "2026-02-01", "until": "2026-02-28"}
        if cursor:
            args["cursor"] = cursor
        resp = await list_orders(args)
        for o in resp["orders"]:
            if o["order_date"].startswith("2026-02") and o["id"] not in seen:
                seen.add(o["id"])
                counts[o["status"]] += 1
        cursor = resp.get("next_cursor")
        if not cursor:
            break
    print(dict(counts))
    await submit_answer({"counts": dict(counts)})
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
