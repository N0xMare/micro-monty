text = await read_file({"path": "/etc/app.conf"})

def convert(v):
    v = v.strip()
    if v.isdigit():
        return int(v)
    if v == "true":
        return True
    if v == "false":
        return False
    return v

config = {}
current = config
for raw in text.splitlines():
    line = raw.strip()
    if not line or line.startswith("#") or line.startswith(";"):
        continue
    if line.startswith("[") and line.endswith("]"):
        name = line[1:-1].strip()
        current = config.setdefault(name, {})
        continue
    if "=" not in line:
        print("skipping unparsed line:", repr(raw))
        continue
    key, value = line.split("=", 1)
    current[key.strip()] = convert(value)

import json
print(json.dumps(config, indent=2))
print(await submit_answer({"config": config}))
