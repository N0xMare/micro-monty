text = await read_file({"path": "/etc/app.conf"})
config = {}
current = config
for raw in text.splitlines():
    line = raw.strip()
    if not line or line.startswith("#"):
        continue
    if line.startswith("[") and line.endswith("]"):
        current = config.setdefault(line[1:-1].strip(), {})
        continue
    key, value = line.split("=", 1)
    key, value = key.strip(), value.strip()
    if value.isdigit():
        value = int(value)
    elif value == "true":
        value = True
    elif value == "false":
        value = False
    current[key] = value
await submit_answer({"config": config})
