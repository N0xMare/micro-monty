async def __codemode_main():
    global asyncio, fetch, found, i, ids, not_found, r, results, transient
    import asyncio

    ids = [4, 77, 12, 300, 25]

    def transient(e):
        return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

    async def fetch(cid):
        for attempt in range(4):
            try:
                return await get_customer({"customer_id": cid})
            except LookupError:
                raise  # permanent: 404, do not retry
            except Exception as e:
                if not transient(e) or attempt == 3:
                    raise
                await asyncio.sleep(0.5 * 2 ** attempt)

    results = await asyncio.gather(*(fetch(i) for i in ids), return_exceptions=True)

    found, not_found = {}, []
    for i, r in zip(ids, results):
        if isinstance(r, LookupError):
            not_found.append(i)
        elif isinstance(r, BaseException):
            raise r
        else:
            found[str(i)] = r["name"]

    print(found, not_found)
    print(await submit_answer({"found": found, "not_found": not_found}))
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
