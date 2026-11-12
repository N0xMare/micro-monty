import asyncio

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def search(query, page):
    for attempt in range(4):  # 1 try + 3 retries
        try:
            return await search_issues({"query": query, "page": page})
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            await asyncio.sleep(0.5 * 2 ** attempt)

keys = set()
page = 1
while True:
    res = await search("priority:P1", page)
    for issue in res["issues"]:
        if issue["priority"] == "P1":
            keys.add(issue["key"])
    if not res.get("has_more") or not res["issues"]:
        break
    page += 1

keys = sorted(keys)
print(len(keys), "P1 issues over", page, "pages:", keys)
print(await submit_answer({"keys": keys}))
