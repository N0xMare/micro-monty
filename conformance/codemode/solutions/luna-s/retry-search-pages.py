async def search(page):
    for attempt in range(4):
        try:
            return await search_issues({"query": "", "page": page})
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                raise
keys = []
page = 1
while True:
    result = await search(page)
    keys.extend(item["key"] for item in result["issues"] if item["priority"] == "P1")
    if not result["has_more"]:
        break
    page += 1
await submit_answer({"keys": sorted(keys)})
