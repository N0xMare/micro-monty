import hashlib

content = await read_file({"path": "/docs/CHANGELOG.md"})
data = content.encode("utf-8")
etag = 'W/"' + hashlib.sha1(data).hexdigest()[:16] + '"'
await submit_answer({"etag": etag, "bytes": len(data)})
