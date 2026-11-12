const issues = [];
let page = 1;
while (true) {
  const res = await tools.search_issues({ query: "status:in_progress", page });
  issues.push(...res.issues);
  if (!res.has_more) break;
  page += 1;
}
const details = await Promise.all(issues.map((i) => tools.get_issue({ key: i.key })));

const closed = [];
for (const d of [...details].sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0))) {
  if (d.status !== "in_progress") continue;
  const comments = d.comments ?? [];
  if (comments.length && comments[comments.length - 1].body.startsWith("Fixed in #")) {
    await tools.update_issue({ key: d.key, fields: { status: "closed" } });
    await tools.add_comment({ key: d.key, body: "Closing: fix merged" });
    closed.push(d.key);
  }
}
console.log("checked:", details.length, "closed:", closed);
if ("submit_answer" in tools) await tools.submit_answer({ closed });
