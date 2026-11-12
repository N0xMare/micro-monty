import base64, json

response = json.loads(await http_get({"url": "https://api.example.com/v1/token"}))
token = json.loads(response["body"])["access_token"]
part = token.split(".")[1]
claims = json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))
await submit_answer({"sub": claims["sub"], "scopes": claims["scope"].split(), "lifetime_s": claims["exp"] - claims["iat"]})
