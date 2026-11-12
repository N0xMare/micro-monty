import hashlib

content = await read_file({"path": "/docs/CHANGELOG.md"})
b = content.encode("utf-8")
etag = 'W/"' + hashlib.sha1(b).hexdigest()[:16] + '"'
print(etag, len(b))
print(await submit_answer({"etag": etag, "bytes": len(b)}))
