import hmac, hashlib

body = '{"event":"invoice.paid","id":"evt_123","amount":4200}'
header = "sha256=7e65a3b4df25cf710a19aa085ad180d14654b636800a652a219c275855086fa4"
secret = "whsec_test_5f1c"

expected = hmac.new(secret.encode("utf-8"), body.encode("utf-8"), hashlib.sha256).hexdigest()
received = header.split("=", 1)[1]
valid = hmac.compare_digest(expected, received.lower())
print(valid, expected)
print(await submit_answer({"valid": valid, "expected_hex": expected}))
