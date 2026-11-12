import tomllib

text = await read_file({"path": "/etc/settings.toml"})
cfg = tomllib.loads(text)

server = cfg["server"]
features = cfg.get("features", [])
enabled = [f["name"] for f in features if f.get("enabled") is True]

answer = {
    "port": int(server["port"]),
    "workers": int(server["workers"]),
    "enabled_features": enabled,
}
print(answer)
print(await submit_answer(answer))
