DO $$
DECLARE
    target_schema TEXT := current_schema();
    max_stream_id BIGINT := 0;
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind = 'S'
          AND n.nspname = target_schema
          AND c.relname = 'to_device_stream_id_seq'
    ) THEN
        EXECUTE format('CREATE SEQUENCE %I.to_device_stream_id_seq', target_schema);
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = target_schema
          AND table_name = 'to_device_messages'
          AND column_name = 'stream_id'
    ) THEN
        EXECUTE format(
            'SELECT COALESCE(MAX(stream_id), 0) FROM %I.to_device_messages',
            target_schema
        )
        INTO max_stream_id;

        PERFORM setval(
            format('%I.to_device_stream_id_seq', target_schema)::regclass,
            GREATEST(max_stream_id, 1),
            max_stream_id > 0
        );
    END IF;
END $$;
