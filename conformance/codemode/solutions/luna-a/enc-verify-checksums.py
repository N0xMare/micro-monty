import hashlib
manifest = await read_file({"path": "/release/SHA256SUMS"})
mismatched, checked = [], 0
for line in manifest.splitlines():
    if not line.strip(): continue
    expected, name = line.split(None, 1); name = name.lstrip(" *")
    content = (await read_file({"path": "/release/" + name})).encode("utf-8")
    checked += 1
    if hashlib.sha256(content).hexdigest().lower() != expected.lower(): mismatched.append(name)
await submit_answer({"mismatched": sorted(mismatched), "checked": checked})
