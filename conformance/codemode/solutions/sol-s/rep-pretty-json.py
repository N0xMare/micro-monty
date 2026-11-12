import json

hosts = await list_hosts({})
data = {"hosts": len(hosts), "zones": sorted({h["zone"] for h in hosts})}
await write_file({"path": "/out/cluster.json", "content": json.dumps(data, indent=2)})
