import psycopg2
conn = psycopg2.connect("postgresql://synapse:synapse@localhost:5432/synapse_test")
conn.autocommit = True
cur = conn.cursor()
cur.execute("ALTER TABLE access_tokens ALTER COLUMN token DROP NOT NULL")
print("Fixed token NOT NULL")
cur.execute("ALTER TABLE access_tokens DROP CONSTRAINT IF EXISTS uq_access_tokens_token")
print("Dropped old constraint")
try:
    cur.execute("ALTER TABLE access_tokens ADD CONSTRAINT uq_access_tokens_token_hash UNIQUE (token_hash)")
    print("Added token_hash unique")
except Exception as e:
    print(f"token_hash unique: {e}")
try:
    cur.execute("ALTER TABLE refresh_tokens ADD CONSTRAINT uq_refresh_tokens_token_hash UNIQUE (token_hash)")
    print("Added refresh token_hash unique")
except Exception as e:
    print(f"refresh token_hash unique: {e}")
try:
    cur.execute("ALTER TABLE token_blacklist ADD CONSTRAINT uq_token_blacklist_token_hash UNIQUE (token_hash)")
    print("Added blacklist token_hash unique")
except Exception as e:
    print(f"blacklist token_hash unique: {e}")
cur.close()
conn.close()
