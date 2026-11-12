import json

async def search_with_retries(page):
    for attempt in range(4):
        try:
            return json.loads(await search_issues({"query": "priority:P1", "page": page}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise

issues = []
page = 1
while True:
    result = await search_with_retries(page)
    issues.extend(result["issues"])
    if not result["has_more"]:
        break
    page += 1
await submit_answer({"keys": sorted(issue["key"] for issue in issues)})
