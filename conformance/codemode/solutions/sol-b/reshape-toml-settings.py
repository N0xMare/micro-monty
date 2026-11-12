import tomllib

data = tomllib.loads(await read_file({"path": "/etc/settings.toml"}))
await submit_answer({"port": data["server"]["port"], "workers": data["server"]["workers"], "enabled_features": [f["name"] for f in data["features"] if f["enabled"]]})
