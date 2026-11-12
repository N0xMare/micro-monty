import asyncio, json

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            # LookupError (404) is permanent: never retried
            if isinstance(e, LookupError) or not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)

ids = [4, 77, 12, 300, 25]
results = await asyncio.gather(*[with_retry(get_customer, {"customer_id": i}) for i in ids], return_exceptions=True)

found, not_found = {}, []
for i, r in zip(ids, results):
    if isinstance(r, LookupError):
        not_found.append(i)
    elif isinstance(r, BaseException):
        raise r
    else:
        found[str(i)] = json.loads(r)["name"]

print("found:", found, "not_found:", not_found)
print(await submit_answer({"found": found, "not_found": not_found}))
