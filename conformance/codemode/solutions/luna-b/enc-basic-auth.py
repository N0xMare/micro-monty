import base64

header = "Basic " + base64.b64encode(b"svc-reporter:s3cr3t:pa55").decode("ascii")
decoded_user, decoded_password = base64.b64decode("YWRtaW46aHVudGVyMg==").decode("utf-8").split(":", 1)
await submit_answer({"header": header, "decoded_user": decoded_user, "decoded_password": decoded_password})
