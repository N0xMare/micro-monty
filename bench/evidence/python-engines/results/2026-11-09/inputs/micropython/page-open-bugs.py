async def __codemode_main():
    global i, keys, page, resp, result
    keys = set()
    page = 1
    while True:
        resp = await search_issues({"query": "label:bug", "page": page})
        for i in resp["issues"]:
            if "bug" in i["labels"] and i["status"] != "closed":
                keys.add(i["key"])
        if not resp.get("has_more"):
            break
        page += 1
    result = sorted(keys)
    print(len(result), result)
    await submit_answer({"keys": result})
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
