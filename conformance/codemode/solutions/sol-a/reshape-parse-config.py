text = await read_file({"path": "/etc/app.conf"})
config = {}
current = config
for raw in text.splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line:
        continue
    if line.startswith("[") and line.endswith("]"):
        current = config.setdefault(line[1:-1].strip(), {})
    else:
        key, value = (part.strip() for part in line.split("=", 1))
        if value.isdigit():
            value = int(value)
        elif value in ("true", "false"):
            value = value == "true"
        current[key] = value
await submit_answer({"config": config})
