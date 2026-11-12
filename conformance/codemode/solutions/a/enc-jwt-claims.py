import json
import base64

resp = json.loads(await http_get({"url": "https://api.example.com/v1/token"}))
print(resp["status"])
body = json.loads(resp["body"])
token = body["access_token"]
part = token.split(".")[1]
part += "=" * (-len(part) % 4)
claims = json.loads(base64.urlsafe_b64decode(part).decode("utf-8"))
print(claims)
scope = claims.get("scope", claims.get("scp", ""))
scopes = scope.split() if isinstance(scope, str) else list(scope)
lifetime = int(claims["exp"]) - int(claims["iat"])
print(await submit_answer({"sub": str(claims["sub"]), "scopes": scopes, "lifetime_s": lifetime}))
