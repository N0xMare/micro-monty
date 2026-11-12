text = await read_file({"path": "/etc/app.conf"})
config = {}
section = config
for raw in text.splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line:
        continue
    if line.startswith("[") and line.endswith("]"):
        section = config.setdefault(line[1:-1].strip(), {})
        continue
    key, value = (part.strip() for part in line.split("=", 1))
    if value.isdecimal():
        value = int(value)
    elif value in ("true", "false"):
        value = value == "true"
    section[key] = value
await submit_answer({"config": config})
