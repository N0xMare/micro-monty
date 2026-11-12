import json, re

text = await read_file({"path": "/etc/app.conf"})
config = {}
current = config
for line in text.splitlines():
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    if line.startswith("[") and line.endswith("]"):
        section = line[1:-1].strip()
        current = config.setdefault(section, {})
        continue
    key, value = line.split("=", 1)
    value = value.strip()
    if re.fullmatch(r"\d+", value):
        parsed = int(value)
    elif value.lower() in ("true", "false"):
        parsed = value.lower() == "true"
    else:
        parsed = value
    current[key.strip()] = parsed
await submit_answer({"config": config})
