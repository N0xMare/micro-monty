import json
text = await fetch_logs({"service": "auth", "date": "2026-03-14"})
print(text["lines"])
