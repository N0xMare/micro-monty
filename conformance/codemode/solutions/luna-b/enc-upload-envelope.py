import base64, hashlib, json

content = (await read_file({"path": "/docs/contract.txt"})).encode("utf-8")
envelope = {"name": "contract.txt", "size": len(content), "sha256": hashlib.sha256(content).hexdigest(), "content_b64": base64.b64encode(content).decode("ascii")}
await write_file({"path": "/out/upload.json", "content": json.dumps(envelope, separators=(",", ":"))})
