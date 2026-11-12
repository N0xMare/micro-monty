import hashlib
data = (await read_file({"path": "/docs/CHANGELOG.md"})).encode("utf-8")
await submit_answer({"etag": 'W/"' + hashlib.sha1(data).hexdigest()[:16] + '"', "bytes": len(data)})
