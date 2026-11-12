import json
hosts = await list_hosts({})
await write_file({"path": "/out/cluster.json", "content": json.dumps({"hosts": len(hosts), "zones": sorted({h["zone"] for h in hosts})}, indent=2) + "\n"})
