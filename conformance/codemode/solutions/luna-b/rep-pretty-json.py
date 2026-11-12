import json

hosts = json.loads(await list_hosts({}))
output = {"hosts": len(hosts), "zones": sorted({host["zone"] for host in hosts})}
await write_file({"path": "/out/cluster.json", "content": json.dumps(output, indent=2) + "\n"})
