BEGIN;
INSERT INTO chain_assets(chain,symbol,token_contract,decimals,purpose,enabled)
VALUES ('monad_testnet','MON',NULL,18,'BOTH',true)
ON CONFLICT (chain, (lower(COALESCE(token_contract, 'native'))))
DO UPDATE SET symbol=EXCLUDED.symbol,decimals=EXCLUDED.decimals,purpose=EXCLUDED.purpose,enabled=true;
COMMIT;
