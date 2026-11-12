import re

text = await read_file({"path": "/etc/app.conf"})

def convert(v):
    v = v.strip()
    if re.fullmatch(r"[0-9]+", v):
        return int(v)
    if v.lower() == "true":
        return True
    if v.lower() == "false":
        return False
    return v

config = {}
current = config
skipped = []
for n, raw in enumerate(text.splitlines(), 1):
    line = raw.strip()
    if not line or line.startswith("#") or line.startswith(";"):
        continue
    m = re.fullmatch(r"\[\s*(.+?)\s*\]", line)
    if m:
        current = config.setdefault(m.group(1), {})
        continue
    if "=" in line:
        key, val = line.split("=", 1)
        current[key.strip()] = convert(val)
    else:
        skipped.append((n, raw))

if skipped:
    print("unrecognised lines:", skipped)
print(config)
print(await submit_answer({"config": config}))
