import json

async def fetch(page):
    for attempt in range(4):
        try:
            raw = await search_issues({"query": "priority:P1", "page": page})
        except (TimeoutError, RuntimeError) as error:
            if attempt == 3 or not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise
        else:
            return json.loads(raw)

keys = []
page = 1
while True:
    data = await fetch(page)
    keys.extend(issue["key"] for issue in data["issues"])
    if not data["has_more"]:
        break
    page += 1
await submit_answer({"keys": sorted(keys)})
