import json
import tomllib

settings = tomllib.loads(await read_file({"path": "/etc/settings.toml"}))
server = settings["server"]
enabled = [feature["name"] for feature in settings["features"] if feature.get("enabled") is True]
await submit_answer({"port": server["port"], "workers": server["workers"], "enabled_features": enabled})
