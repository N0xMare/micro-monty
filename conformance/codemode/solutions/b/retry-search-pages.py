import asyncio
import json


def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))


async def call(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)


keys = set()
page = 1
while True:
    res = json.loads(await call(search_issues, {"query": "priority:P1", "page": page}))
    keys.update(i["key"] for i in res["issues"] if i.get("priority") == "P1")
    if not res.get("has_more"):
        break
    page += 1


def sort_key(k):
    prefix, _, num = k.rpartition("-")
    return (prefix, int(num)) if num.isdigit() else (k, 0)


result = sorted(keys, key=sort_key)
print(page, "pages;", result)
print(await submit_answer({"keys": result}))
