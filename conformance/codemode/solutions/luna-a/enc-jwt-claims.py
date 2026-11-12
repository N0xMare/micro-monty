import base64
import json
r = json.loads(await http_get({"url": "https://api.example.com/v1/token"}))
part = r["access_token"].split(".")[1]
payload = json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))
scopes = payload.get("scope", payload.get("scopes", []))
if isinstance(scopes, str): scopes = scopes.split()
await submit_answer({"sub": payload["sub"], "scopes": scopes, "lifetime_s": payload["exp"] - payload["iat"]})
