import hmac
import hashlib

body = '{"event":"invoice.paid","id":"evt_123","amount":4200}'
secret = "whsec_test_5f1c"
signature = "sha256=7e65a3b4df25cf710a19aa085ad180d14654b636800a652a219c275855086fa4"
expected = hmac.new(secret.encode("utf-8"), body.encode("utf-8"), hashlib.sha256).hexdigest()
await submit_answer({"valid": hmac.compare_digest(signature, "sha256=" + expected), "expected_hex": expected})
