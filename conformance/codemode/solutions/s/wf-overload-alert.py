issues, page = [], 1
while True:
    res = await search_issues({"query": "assignee:bjorn", "page": page})
    issues.extend(res["issues"])
    if not res.get("has_more"):
        break
    page += 1

total = sum(
    (i.get("estimate_hours") or 0)
    for i in issues
    if i.get("assignee") == "bjorn" and i["status"] != "closed"
)
print("total:", total)

posted = False
if total > 20:
    await post_message({"channel": "#eng", "text": f"bjorn is overloaded ({total}h)"})
    posted = True

if "submit_answer" in globals():
    await submit_answer({"total_hours": total, "posted": posted})
