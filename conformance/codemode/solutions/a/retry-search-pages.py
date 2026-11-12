import asyncio, json

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
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
    res = json.loads(await with_retry(search_issues, {"query": "priority:P1", "page": page}))
    for issue in res["issues"]:
        if issue["priority"] == "P1":
            keys.add(issue["key"])
    if not res.get("has_more"):
        break
    page += 1

def sort_key(k):
    prefix, _, num = k.rpartition("-")
    return (prefix, int(num)) if num.isdigit() else (k, 0)

result = sorted(keys, key=sort_key)
print(page, "pages,", len(result), "P1 issues:", result)
print(await submit_answer({"keys": result}))
