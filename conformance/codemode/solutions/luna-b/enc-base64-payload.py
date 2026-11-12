import base64, json

encoded = await read_file({"path": "/inbox/payload.b64"})
doc = json.loads(base64.b64decode(encoded.strip()))
records = doc["records"]
await submit_answer({"batch": doc["batch"], "records": len(records), "total": round(sum(r["amount"] for r in records), 2)})
