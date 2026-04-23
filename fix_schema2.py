import psycopg2
conn = psycopg2.connect("postgresql://synapse:synapse@localhost:5432/synapse_test")
conn.autocommit = True
cur = conn.cursor()

tables = ["access_tokens", "token_blacklist"]
for table in tables:
    try:
        cur.execute(f"ALTER TABLE {table} ALTER COLUMN token DROP NOT NULL")
        print(f"Fixed {table}.token NOT NULL")
    except Exception as e:
        print(f"{table}.token: {e}")

cur.close()
conn.close()
