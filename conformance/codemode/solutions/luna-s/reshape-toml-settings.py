import tomllib
settings = tomllib.loads(await read_file({"path": "/etc/settings.toml"}))
enabled = [feature["name"] for feature in settings.get("features", []) if feature.get("enabled") is True]
await submit_answer({"port": settings["server"]["port"], "workers": settings["server"]["workers"], "enabled_features": enabled})
