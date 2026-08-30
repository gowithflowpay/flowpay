DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'flowpay') THEN
    CREATE ROLE flowpay LOGIN;
  END IF;
END
$$;
ALTER ROLE flowpay LOGIN PASSWORD 'flowpay';
