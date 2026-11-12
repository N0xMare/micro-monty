import json
import base64
import hashlib

content = await read_file({"path": "/docs/contract.txt"})
b = content.encode("utf-8")
envelope = {
    "name": "contract.txt",
    "size": len(b),
    "sha256": hashlib.sha256(b).hexdigest(),
    "content_b64": base64.b64encode(b).decode("ascii"),
}
res = await write_file({"path": "/out/upload.json", "content": json.dumps(envelope)})
print(res)
print({k: v for k, v in envelope.items() if k != "content_b64"})
