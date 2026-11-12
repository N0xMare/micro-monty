import base64
import json

encoded = await read_file({"path": "/inbox/payload.b64"})
document = json.loads(base64.b64decode(encoded.strip()).decode("utf-8"))
records = document["records"]
await submit_answer({"batch": document["batch"], "records": len(records), "total": round(sum(float(record["amount"]) for record in records), 2)})
