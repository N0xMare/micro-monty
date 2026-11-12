import base64
import json

response = await http_get({"url": "https://api.example.com/v1/token"})
token = json.loads(response["body"])["access_token"]
payload_part = token.split(".")[1]
payload = json.loads(base64.urlsafe_b64decode(payload_part + "=" * (-len(payload_part) % 4)))
scope_claim = payload.get("scope", payload.get("scopes", ""))
scopes = scope_claim.split() if isinstance(scope_claim, str) else scope_claim
await submit_answer({"sub": payload["sub"], "scopes": scopes, "lifetime_s": int(payload["exp"]) - int(payload["iat"])})
