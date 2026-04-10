DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind = 'S'
          AND c.relname = 'to_device_stream_id_seq'
    ) THEN
        CREATE SEQUENCE to_device_stream_id_seq;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.tables
        WHERE table_name = 'to_device_messages'
    ) THEN
        PERFORM setval(
            'to_device_stream_id_seq',
            GREATEST((SELECT COALESCE(MAX(stream_id), 0) FROM to_device_messages), 0)
        );
    END IF;
END $$;
