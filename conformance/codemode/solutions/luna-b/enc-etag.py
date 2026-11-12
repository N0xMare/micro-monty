import hashlib

content = (await read_file({"path": "/docs/CHANGELOG.md"})).encode("utf-8")
etag = 'W/"' + hashlib.sha1(content).hexdigest()[:16] + '"'
await submit_answer({"etag": etag, "bytes": len(content)})
