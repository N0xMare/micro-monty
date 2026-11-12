import base64, json

resp = await http_get({"url": "https://api.example.com/v1/token"})
print(resp["status"], resp["content_type"])
body = json.loads(resp["body"])
token = body["access_token"]
seg = token.split(".")[1]
seg += "=" * (-len(seg) % 4)
claims = json.loads(base64.urlsafe_b64decode(seg).decode("utf-8"))
print(claims)
scope = claims.get("scope", claims.get("scp", ""))
scopes = scope.split() if isinstance(scope, str) else list(scope)
await submit_answer({"sub": str(claims["sub"]), "scopes": scopes, "lifetime_s": int(claims["exp"]) - int(claims["iat"])})
