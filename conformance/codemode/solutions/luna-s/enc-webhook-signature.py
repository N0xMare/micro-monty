import hashlib
import hmac

body = b'{"event":"invoice.paid","id":"evt_123","amount":4200}'
expected = hmac.new(b"whsec_test_5f1c", body, hashlib.sha256).hexdigest()
provided = "7e65a3b4df25cf710a19aa085ad180d14654b636800a652a219c275855086fa4"
await submit_answer({"valid": hmac.compare_digest(expected, provided), "expected_hex": expected})
