#!/usr/bin/env python3
"""
Schema Extraction Script
Extracts database schema from PostgreSQL and generates a JSON representation.
Used for drift detection by comparing with expected schema from migration scripts.
"""

import os
import sys
import json
import argparse
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, asdict
from datetime import datetime


@dataclass
class ColumnInfo:
    name: str
    data_type: str
    is_nullable: bool
    column_default: Optional[str]
    is_primary_key: bool
    is_foreign_key: bool
    foreign_table: Optional[str]
    foreign_column: Optional[str]


@dataclass
class IndexInfo:
    name: str
    columns: List[str]
    is_unique: bool
    is_primary: bool


@dataclass
class TableInfo:
    name: str
    columns: List[ColumnInfo]
    indexes: List[IndexInfo]
    row_count: Optional[int] = None


@dataclass
class SchemaInfo:
    database: str
    extracted_at: str
    tables: Dict[str, TableInfo]
    version: Optional[str] = None


def get_connection_params() -> Dict[str, str]:
    """Get database connection parameters from environment."""
    return {
        'host': os.environ.get('PGHOST', 'localhost'),
        'port': os.environ.get('PGPORT', '5432'),
        'database': os.environ.get('PGDATABASE', 'synapse'),
        'user': os.environ.get('PGUSER', 'synapse'),
        'password': os.environ.get('PGPASSWORD', 'synapse'),
    }


def parse_columns(cursor, table_name: str, schema: str = 'public') -> List[ColumnInfo]:
    """Extract column information for a table."""
    query = """
        SELECT
            c.column_name,
            c.data_type,
            c.is_nullable,
            c.column_default,
            c.character_maximum_length,
            c.numeric_precision,
            c.numeric_scale,
            CASE WHEN tc.constraint_type = 'PRIMARY KEY' THEN TRUE ELSE FALSE END as is_primary_key
        FROM information_schema.columns c
        LEFT JOIN information_schema.key_column_usage kcu
            ON c.table_schema = kcu.table_schema
            AND c.table_name = kcu.table_name
            AND c.column_name = kcu.column_name
        LEFT JOIN information_schema.table_constraints tc
            ON kcu.constraint_schema = tc.constraint_schema
            AND kcu.constraint_name = tc.constraint_name
            AND tc.constraint_type = 'PRIMARY KEY'
        WHERE c.table_schema = %s AND c.table_name = %s
        ORDER BY c.ordinal_position;
    """

    cursor.execute(query, (schema, table_name))
    columns = []

    for row in cursor.fetchall():
        col_name, data_type, is_nullable, default, max_len, num_prec, num_scale, is_pk = row

        if max_len:
            full_type = f"{data_type}({max_len})"
        elif num_prec is not None and num_scale is not None:
            full_type = f"{data_type}({num_prec},{num_scale})"
        elif num_prec is not None:
            full_type = f"{data_type}({num_prec})"
        else:
            full_type = data_type

        columns.append(ColumnInfo(
            name=col_name,
            data_type=full_type,
            is_nullable=is_nullable == 'YES',
            column_default=default,
            is_primary_key=is_pk,
            is_foreign_key=False,
            foreign_table=None,
            foreign_column=None
        ))

    return columns


def parse_foreign_keys(cursor, table_name: str, schema: str = 'public') -> List[tuple]:
    """Extract foreign key relationships."""
    query = """
        SELECT
            kcu.column_name,
            ccu.table_name AS foreign_table_name,
            ccu.column_name AS foreign_column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        JOIN information_schema.constraint_column_usage ccu
            ON ccu.constraint_name = tc.constraint_name
            AND ccu.table_schema = tc.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
            AND tc.table_schema = %s
            AND tc.table_name = %s;
    """

    cursor.execute(query, (schema, table_name))
    return [(row[0], row[1], row[2]) for row in cursor.fetchall()]


def parse_indexes(cursor, table_name: str, schema: str = 'public') -> List[IndexInfo]:
    """Extract index information for a table."""
    query = """
        SELECT
            i.relname AS index_name,
            a.attname AS column_name,
            ix.indisunique AS is_unique,
            ix.indisprimary AS is_primary
        FROM pg_class t
        JOIN pg_namespace ns ON t.relnamespace = ns.oid
        JOIN pg_index ix ON t.oid = ix.indrelid
        JOIN pg_class i ON i.oid = ix.indexrelid
        JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
        WHERE t.relkind = 'r'
            AND ns.nspname = %s
            AND t.relname = %s
        ORDER BY i.relname, a.attnum;
    """

    cursor.execute(query, (schema, table_name))
    rows = cursor.fetchall()

    indexes: Dict[str, IndexInfo] = {}
    for row in rows:
        idx_name, col_name, is_unique, is_primary = row
        if idx_name not in indexes:
            indexes[idx_name] = IndexInfo(
                name=idx_name,
                columns=[],
                is_unique=is_unique,
                is_primary=is_primary
            )
        indexes[idx_name].columns.append(col_name)

    return list(indexes.values())


def get_table_row_count(cursor, table_name: str, schema: str = 'public') -> Optional[int]:
    """Get approximate row count for a table."""
    try:
        query = f"SELECT reltuples::bigint FROM pg_class WHERE relname = %s AND relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = %s)"
        cursor.execute(query, (table_name, schema))
        result = cursor.fetchone()
        return int(result[0]) if result else None
    except Exception:
        return None


def extract_schema(cursor, schema: str = 'public', include_row_counts: bool = True) -> SchemaInfo:
    """Extract complete schema information from database."""
    cursor.execute("""
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = %s
        AND table_type = 'BASE TABLE'
        ORDER BY table_name;
    """, (schema,))

    tables: Dict[str, TableInfo] = {}

    for (table_name,) in cursor.fetchall():
        print(f"Processing table: {table_name}", file=sys.stderr)

        columns = parse_columns(cursor, table_name, schema)

        fks = parse_foreign_keys(cursor, table_name, schema)
        for col in columns:
            for col_name, fk_table, fk_col in fks:
                if col.name == col_name:
                    col.is_foreign_key = True
                    col.foreign_table = fk_table
                    col.foreign_column = fk_col

        indexes = parse_indexes(cursor, table_name, schema)

        row_count = None
        if include_row_counts:
            row_count = get_table_row_count(cursor, table_name, schema)

        tables[table_name] = TableInfo(
            name=table_name,
            columns=columns,
            indexes=indexes,
            row_count=row_count
        )

    return SchemaInfo(
        database=schema,
        extracted_at=datetime.now().isoformat(),
        tables=tables
    )


def load_schema_from_json(json_file: str) -> SchemaInfo:
    """Load schema from JSON file (generated from migration scripts)."""
    with open(json_file, 'r') as f:
        data = json.load(f)

    tables = {}
    for table_name, table_data in data.get('tables', {}).items():
        columns = [ColumnInfo(**col) for col in table_data.get('columns', [])]
        indexes = [IndexInfo(**idx) for idx in table_data.get('indexes', [])]
        tables[table_name] = TableInfo(
            name=table_name,
            columns=columns,
            indexes=indexes,
            row_count=table_data.get('row_count')
        )

    return SchemaInfo(
        database=data.get('database', 'unknown'),
        extracted_at=data.get('extracted_at', ''),
        tables=tables,
        version=data.get('version')
    )


def schema_to_json(schema: SchemaInfo, output_file: str):
    """Save schema to JSON file."""
    data = {
        'database': schema.database,
        'extracted_at': schema.extracted_at,
        'version': schema.version,
        'tables': {}
    }

    for table_name, table in schema.tables.items():
        data['tables'][table_name] = {
            'columns': [asdict(col) for col in table.columns],
            'indexes': [asdict(idx) for idx in table.indexes],
            'row_count': table.row_count
        }

    with open(output_file, 'w') as f:
        json.dump(data, f, indent=2)


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description='Extract database schema')
    parser.add_argument('--host', default=os.environ.get('PGHOST', 'localhost'))
    parser.add_argument('--port', default=os.environ.get('PGPORT', '5432'))
    parser.add_argument('--database', default=os.environ.get('PGDATABASE', 'synapse'))
    parser.add_argument('--user', default=os.environ.get('PGUSER', 'synapse'))
    parser.add_argument('--password', default=os.environ.get('PGPASSWORD', 'synapse'))
    parser.add_argument('--schema', default='public')
    parser.add_argument('--output', '-o', required=True, help='Output JSON file')
    parser.add_argument('--skip-row-counts', action='store_true', help='Skip row counts')

    args = parser.parse_args()

    try:
        import psycopg2
    except ImportError:
        try:
            import pg8000
        except ImportError:
            print("Error: Neither psycopg2 nor pg8000 is installed", file=sys.stderr)
            sys.exit(1)

    try:
        conn = psycopg2.connect(
            host=args.host,
            port=args.port,
            database=args.database,
            user=args.user,
            password=args.password
        )
    except Exception as e:
        print(f"Error connecting to database: {e}", file=sys.stderr)
        sys.exit(1)

    cursor = conn.cursor()
    schema = extract_schema(cursor, args.schema, not args.skip_row_counts)
    schema_to_json(schema, args.output)

    cursor.close()
    conn.close()

    print(f"Schema extracted to {args.output}", file=sys.stderr)
    print(f"Total tables: {len(schema.tables)}", file=sys.stderr)


if __name__ == '__main__':
    main()
