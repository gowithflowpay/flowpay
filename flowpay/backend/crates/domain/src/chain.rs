use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainKey {
    Base,
    Bsc,
    Solana,
    Custom(String),
}

impl fmt::Display for ChainKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base => f.write_str("base"),
            Self::Bsc => f.write_str("bsc"),
            Self::Solana => f.write_str("solana"),
            Self::Custom(value)
                if matches!(
                    value.as_str(),
                    "bsc_testnet"
                        | "ethereum_sepolia"
                        | "base_sepolia"
                        | "arbitrum_sepolia"
                        | "optimism_sepolia"
                        | "polygon_amoy"
                ) =>
            {
                f.write_str(value)
            }
            Self::Custom(value) => write!(f, "custom:{value}"),
        }
    }
}

#[derive(Debug, Error)]
#[error("unsupported chain: {0}")]
pub struct ChainParseError(pub String);

impl FromStr for ChainKey {
    type Err = ChainParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "base" => Ok(Self::Base),
            "bsc" | "bnb" | "bnb-smart-chain" => Ok(Self::Bsc),
            "bsc_testnet" | "bnb_testnet" | "bnb-smart-chain-testnet" => {
                Ok(Self::Custom("bsc_testnet".to_owned()))
            }
            "solana" => Ok(Self::Solana),
            "ethereum_sepolia" | "sepolia" => Ok(Self::Custom("ethereum_sepolia".to_owned())),
            "base_sepolia" => Ok(Self::Custom("base_sepolia".to_owned())),
            "arbitrum_sepolia" => Ok(Self::Custom("arbitrum_sepolia".to_owned())),
            "optimism_sepolia" => Ok(Self::Custom("optimism_sepolia".to_owned())),
            "polygon_amoy" => Ok(Self::Custom("polygon_amoy".to_owned())),
            other if other.starts_with("custom:") => Ok(Self::Custom(other[7..].to_owned())),
            _ => Err(ChainParseError(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddressRef {
    pub chain: ChainKey,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_chain_display_round_trips_through_persistence_string() {
        let original = ChainKey::Custom("polygon".into());
        let stored = original.to_string();
        assert_eq!(stored, "custom:polygon");
        assert_eq!(ChainKey::from_str(&stored).unwrap(), original);
    }

    #[test]
    fn known_testnet_chain_uses_canonical_database_key() {
        let chain = ChainKey::from_str("base_sepolia").unwrap();
        assert_eq!(chain.to_string(), "base_sepolia");
        assert_eq!(ChainKey::from_str(&chain.to_string()).unwrap(), chain);
    }
}
