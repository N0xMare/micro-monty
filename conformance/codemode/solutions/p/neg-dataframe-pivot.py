import pandas as pd
import io
df = pd.read_csv(io.StringIO(await read_file({"path": "/data/sales_2026q1.csv"})))
