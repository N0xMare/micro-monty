import json, base64

raw = await read_file({"path": "/inbox/payload.b64"})
b64 = "".join(raw.split())
b64 += "=" * (-len(b64) % 4)
try:
    data = base64.b64decode(b64, validate=False)
except Exception:
    data = base64.urlsafe_b64decode(b64)
doc = json.loads(data.decode("utf-8"))
print(json.dumps(doc)[:1500])

def find_key(obj, names):
    if isinstance(obj, dict):
        for n in names:
            if n in obj:
                return obj[n]
        for v in obj.values():
            r = find_key(v, names)
            if r is not None:
                return r
    return None

batch = find_key(doc, ["batch", "batch_id", "batchId", "id"])
records = find_key(doc, ["records", "items", "entries", "rows"])
if isinstance(batch, dict):
    batch = batch.get("id", batch)
if records is None and isinstance(doc, list):
    records = doc
total = sum(float(r.get("amount", 0)) for r in records)
answer = {"batch": str(batch), "records": len(records), "total": round(total, 2)}
print(answer)
print(await submit_answer(answer))
