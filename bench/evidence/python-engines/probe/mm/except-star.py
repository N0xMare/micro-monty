try:
    raise ExceptionGroup("g", [ValueError(1)])
except* ValueError:
    r = "caught"
print(r)
