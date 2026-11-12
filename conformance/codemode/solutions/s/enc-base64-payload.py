import base64, binascii, json

raw = await read_file({"path": "/inbox/payload.b64"})
s = "".join(raw.split())
if s.startswith("data:") and "," in s:
    s = s.split(",", 1)[1]
s += "=" * (-len(s) % 4)
try:
    data = base64.b64decode(s, validate=True)
except binascii.Error:
    data = base64.urlsafe_b64decode(s)
doc = json.loads(data.decode("utf-8-sig"))
print(json.dumps(doc, indent=1)[:2000])

def find(d, names):
    for n in names:
        if isinstance(d, dict) and n in d:
            return d[n]
    return None

batch = find(doc, ["batch", "batch_id", "batchId", "id"])
if isinstance(batch, dict):
    inner = batch
    batch = find(inner, ["id", "batch_id", "batchId"])
    records = find(inner, ["records", "items"]) or find(doc, ["records", "items"])
else:
    records = find(doc, ["records", "items", "data"])
if isinstance(records, dict):
    records = list(records.values())
records = records or []
total = 0.0
for r in records:
    a = find(r, ["amount", "amt", "value"])
    if isinstance(a, dict):
        a = find(a, ["value", "amount"])
    if a is not None:
        total += float(a)
print(batch, len(records), total)
await submit_answer({"batch": str(batch), "records": len(records), "total": round(total, 2)})
