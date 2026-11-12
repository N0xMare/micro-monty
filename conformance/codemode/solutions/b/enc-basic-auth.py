import base64

header = "Basic " + base64.b64encode("svc-reporter:s3cr3t:pa55".encode("utf-8")).decode("ascii")
decoded = base64.b64decode("Basic YWRtaW46aHVudGVyMg==".split(" ", 1)[1]).decode("utf-8")
user, password = decoded.split(":", 1)

answer = {"header": header, "decoded_user": user, "decoded_password": password}
print(answer)
print(await submit_answer(answer))
