x = []
for _ in range(1000000):
    x = [x]
import json
print(len(json.dumps(x)))
