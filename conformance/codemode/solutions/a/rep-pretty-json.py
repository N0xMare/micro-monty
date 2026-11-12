import json

hosts = json.loads(await list_hosts({}))
data = {"hosts": len(hosts), "zones": sorted({h["zone"] for h in hosts})}
content = json.dumps(data, indent=2) + "\n"
print(content)
print(await write_file({"path": "/out/cluster.json", "content": content}))
