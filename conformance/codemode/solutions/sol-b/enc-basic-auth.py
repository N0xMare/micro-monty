import base64
header = "Basic " + base64.b64encode("svc-reporter:s3cr3t:pa55".encode("utf-8")).decode("ascii")
user, password = base64.b64decode("YWRtaW46aHVudGVyMg==").decode("utf-8").split(":", 1)
await submit_answer({"header": header, "decoded_user": user, "decoded_password": password})
