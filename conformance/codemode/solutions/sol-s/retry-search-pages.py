async def fetch(page):
    for attempt in range(4):
        try:
            return await search_issues({"query": "priority:P1", "page": page})
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)) or attempt == 3:
                raise

keys = []
page = 1
while True:
    result = await fetch(page)
    keys.extend(i["key"] for i in result["issues"])
    if not result["has_more"]:
        break
    page += 1
await submit_answer({"keys": sorted(keys)})
