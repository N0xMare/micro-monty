import csv, io

text = await read_file({"path": "/data/survey.csv"})
rows = list(csv.DictReader(io.StringIO(text, newline="")))

def rid(v):
    v = v.strip()
    return int(v) if v.isdigit() else v

scores = []
with_comment = 0
multiline = []
for r in rows:
    s = (r.get("score") or "").strip()
    if s:
        scores.append(float(s))
    c = r.get("comment") or ""
    if c.strip():
        with_comment += 1
    if "\n" in c.strip() or "\r" in c.strip():
        multiline.append(rid(r["respondent"]))

summary = {
    "responses": len(rows),
    "average_score": round(sum(scores) / len(scores), 2) if scores else 0,
    "with_comment": with_comment,
    "multiline": sorted(multiline),
}
print(summary, "scored:", len(scores))
print(await submit_answer(summary))
