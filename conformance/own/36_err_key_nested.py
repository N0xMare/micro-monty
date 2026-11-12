config = {"db": {"host": "localhost"}}

def port(cfg):
    return cfg["db"]["port"]

def main():
    value = port(config)
    print(value)

main()
