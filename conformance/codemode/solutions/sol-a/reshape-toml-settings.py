import tomllib

settings = tomllib.loads(await read_file({"path": "/etc/settings.toml"}))
await submit_answer({"port": settings["server"]["port"], "workers": settings["server"]["workers"], "enabled_features": [f["name"] for f in settings["features"] if f["enabled"]]})
