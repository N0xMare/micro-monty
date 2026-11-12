import base64
import hashlib
import json

content = await read_file({"path": "/docs/contract.txt"})
data = content.encode("utf-8")
envelope = {"name": "contract.txt", "size": len(data), "sha256": hashlib.sha256(data).hexdigest(), "content_b64": base64.b64encode(data).decode("ascii")}
await write_file({"path": "/out/upload.json", "content": json.dumps(envelope)})
