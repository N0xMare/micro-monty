import json

keys = set()
page = 1
while True:
    res = json.loads(await search_issues({"query": "label:bug", "page": page}))
    for i in res["issues"]:
        if "bug" in i["labels"] and i["status"] != "closed":
            keys.add(i["key"])
    if not res.get("has_more") or not res["issues"]:
        break
    page += 1

def sort_key(k):
    prefix, _, num = k.rpartition("-")
    return (prefix, int(num)) if num.isdigit() else (k, 0)

result = sorted(keys, key=sort_key)
print(len(result), result)
await submit_answer({"keys": result})
