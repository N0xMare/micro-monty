import json

content = await read_file({"path": "/etc/app.conf"})
config = {}
current = config
for raw_line in content.splitlines():
    line = raw_line.strip()
    if not line or line.startswith("#"):
        continue
    if line.startswith("[") and line.endswith("]"):
        section = line[1:-1].strip()
        current = config.setdefault(section, {})
        continue
    key, value = line.split("=", 1)
    key, value = key.strip(), value.strip()
    if value.isdigit():
        parsed = int(value)
    elif value == "true":
        parsed = True
    elif value == "false":
        parsed = False
    else:
        parsed = value
    current[key] = parsed
await submit_answer({"config": config})
