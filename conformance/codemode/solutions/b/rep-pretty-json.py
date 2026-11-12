import json

hosts = json.loads(await list_hosts({}))
data = {"hosts": len(hosts), "zones": sorted({h["zone"] for h in hosts})}
content = json.dumps(data, indent=2) + "\n"

print(content)
print(await write_file({"path": "/out/cluster.json", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer(data))
