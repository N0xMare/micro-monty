import tomllib

settings = tomllib.loads(await read_file({"path": "/etc/settings.toml"}))
server = settings["server"]
features = settings.get("features", [])
enabled = [f["name"] for f in features if f.get("enabled") is True]

answer = {"port": int(server["port"]), "workers": int(server["workers"]), "enabled_features": enabled}
print(answer)
print(await submit_answer(answer))
