issues = []
page_num = 1
while True:
    result = await search_issues({"query": "label:bug", "page": page_num})
    issues.extend(i for i in result["issues"] if i["status"] != "closed" and "bug" in i["labels"])
    if not result["has_more"]:
        break
    page_num += 1
await submit_answer({"keys": sorted(i["key"] for i in issues)})
