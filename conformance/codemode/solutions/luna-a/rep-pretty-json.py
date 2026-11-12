import json

hosts = json.loads(await list_hosts({}))
result = {"hosts": len(hosts), "zones": sorted({host["zone"] for host in hosts})}
await write_file({"path": "/out/cluster.json", "content": json.dumps(result, indent=2) + "\n"})
