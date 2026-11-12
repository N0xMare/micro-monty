import subprocess
print(subprocess.run(["wc", "/docs/contract.txt"], capture_output=True, text=True).stdout)
