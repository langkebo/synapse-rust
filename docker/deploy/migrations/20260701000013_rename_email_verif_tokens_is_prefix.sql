-- Rename email_verification_tokens.used to is_used for v10 is_ prefix alignment
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'email_verification_tokens'
          AND column_name = 'used'
    ) THEN
        ALTER TABLE email_verification_tokens RENAME COLUMN used TO is_used;
    END IF;
END $$;
