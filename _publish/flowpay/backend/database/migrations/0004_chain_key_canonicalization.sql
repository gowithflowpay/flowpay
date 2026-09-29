BEGIN;

UPDATE chain_assets SET chain = lower(chain);
ALTER TABLE chain_assets
  ADD CONSTRAINT chain_assets_chain_key_canonical CHECK (chain = lower(chain));

COMMIT;
