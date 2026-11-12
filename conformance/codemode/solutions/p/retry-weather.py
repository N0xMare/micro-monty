import json
raw = get_weather({"city": "Madrid"})
print(json.loads(raw)["temp_c"])
