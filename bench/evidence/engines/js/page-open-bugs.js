const keys = new Set();
let page = 1;
while (true) {
  const resp = await tools.search_issues({ query: "label:bug", page });
  for (const i of resp.issues) {
    if (i.labels.includes("bug") && i.status !== "closed") keys.add(i.key);
  }
  if (!resp.has_more) break;
  page += 1;
}
const result = [...keys].sort();
console.log(result.length, result);
await tools.submit_answer({ keys: result });
