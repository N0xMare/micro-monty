import asyncio, json

async def retry_search(page):
    for attempt in range(4):
        try:
            return json.loads(await search_issues({"query": "priority:P1", "page": page}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

response = await retry_search(1)
issues = list(response["issues"])
page = 2
while response["has_more"]:
    response = await retry_search(page)
    issues.extend(response["issues"])
    page += 1
keys = sorted({issue["key"] for issue in issues if issue["priority"] == "P1"})
await submit_answer({"keys": keys})
