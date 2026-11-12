import json, hashlib, base64

content = await read_file({"path": "/docs/contract.txt"})
data = content.encode("utf-8")
envelope = {
    "name": "contract.txt",
    "size": len(data),
    "sha256": hashlib.sha256(data).hexdigest(),
    "content_b64": base64.b64encode(data).decode("ascii"),
}
print({k: v for k, v in envelope.items() if k != "content_b64"})
print(await write_file({"path": "/out/upload.json", "content": json.dumps(envelope)}))
