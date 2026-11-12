import json

keys = []
page_no = 1
while True:
    page = json.loads(await search_issues({"query": "label:bug", "page": page_no}))
    keys.extend(i["key"] for i in page["issues"] if "bug" in i["labels"] and i["status"] != "closed")
    if not page.get("has_more"):
        break
    page_no += 1

keys = sorted(set(keys))
print(len(keys), keys)
print(await submit_answer({"keys": keys}))
