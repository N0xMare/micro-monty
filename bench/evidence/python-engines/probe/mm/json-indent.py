import json
r = json.dumps({'a': [1, 2]}, indent=2, sort_keys=True)
print(r)
