import asyncio
import json


def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))


async def call(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            # LookupError (404) is permanent: re-raised immediately, never retried.
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)


ids = [4, 77, 12, 300, 25]
results = await asyncio.gather(*(call(get_customer, {"customer_id": i}) for i in ids), return_exceptions=True)

found, not_found = {}, []
for i, r in zip(ids, results):
    if isinstance(r, LookupError):
        not_found.append(i)
    elif isinstance(r, BaseException):
        raise r
    else:
        found[str(i)] = json.loads(r)["name"]

print(found, not_found)
print(await submit_answer({"found": found, "not_found": not_found}))
