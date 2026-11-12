import base64

header = "Basic " + base64.b64encode("svc-reporter:s3cr3t:pa55".encode("utf-8")).decode("ascii")
username, password = base64.b64decode("YWRtaW46aHVudGVyMg==").decode("utf-8").split(":", 1)
await submit_answer({"header": header, "decoded_user": username, "decoded_password": password})
