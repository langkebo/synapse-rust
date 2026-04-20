import json
import pathlib


BASE = pathlib.Path("/Users/ljf/Desktop/hu/synapse-rust/test-results")
ROLES = ["super_admin", "admin", "user"]


def load_records():
    records = {}
    for role in ROLES:
        path = BASE / role / "api-integration.responses.jsonl"
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            rec = json.loads(line)
            records.setdefault(rec["case"], {})[role] = {
                "outcome": rec.get("outcome", ""),
                "status": rec.get("http_status", ""),
                "reason": rec.get("reason", ""),
                "method": rec.get("http_method", ""),
                "url": rec.get("url", ""),
            }
    return records


def main():
    records = load_records()
    rows = []
    for case, role_map in sorted(records.items()):
        outcomes = tuple(role_map.get(role, {}).get("outcome", "") for role in ROLES)
        statuses = tuple(role_map.get(role, {}).get("status", "") for role in ROLES)
        if len(set(outcomes)) > 1 or any(o and o != "pass" for o in outcomes) or len(set(statuses)) > 1:
            rows.append((case, role_map))

    out = BASE / "role_matrix.tsv"
    with out.open("w", encoding="utf-8") as fh:
        fh.write("case\tmethod\turl\tsuper_admin\tadmin\tuser\n")
        for case, role_map in rows:
            method = next((role_map[r]["method"] for r in ROLES if r in role_map), "")
            url = next((role_map[r]["url"] for r in ROLES if r in role_map), "")
            values = []
            for role in ROLES:
                item = role_map.get(role, {})
                values.append(
                    f"{item.get('outcome','')}/{item.get('status','')}/{item.get('reason','')}"
                )
            fh.write("\t".join([case, method, url, *values]) + "\n")

    print(f"wrote {out}")
    print(f"rows={len(rows)}")
    for case, role_map in rows[:80]:
        print(case)
        for role in ROLES:
            item = role_map.get(role, {})
            print(
                " ",
                role,
                item.get("outcome", ""),
                item.get("status", ""),
                item.get("reason", ""),
            )


if __name__ == "__main__":
    main()
