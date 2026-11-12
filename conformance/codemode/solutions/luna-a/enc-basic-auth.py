import base64
header = "Basic " + base64.b64encode(b"svc-reporter:s3cr3t:pa55").decode("ascii")
decoded = base64.b64decode("YWRtaW46aHVudGVyMg==").decode("utf-8")
user, password = decoded.split(":", 1)
await submit_answer({"header": header, "decoded_user": user, "decoded_password": password})
