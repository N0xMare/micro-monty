import base64
import json

response = await http_get({"url": "https://api.example.com/v1/token"})
token = json.loads(response["body"])["access_token"]
segment = token.split(".")[1]
payload = json.loads(base64.urlsafe_b64decode(segment + "=" * (-len(segment) % 4)))
await submit_answer({"sub": payload["sub"], "scopes": payload["scope"].split(), "lifetime_s": payload["exp"] - payload["iat"]})
