use num_bigint::BigUint;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct AtomicAmount(BigUint);

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AmountError {
    #[error("atomic amount must be an unsigned base-10 integer")]
    Invalid,
    #[error("decimal amount has more fractional digits than the asset supports")]
    TooManyDecimals,
}

impl AtomicAmount {
    #[must_use]
    pub fn zero() -> Self {
        Self(BigUint::default())
    }

    #[must_use]
    pub fn inner(&self) -> &BigUint {
        &self.0
    }

    #[must_use]
    pub fn from_biguint(value: BigUint) -> Self {
        Self(value)
    }

    pub fn from_hex_quantity(value: &str) -> Result<Self, AmountError> {
        let hex = value.strip_prefix("0x").unwrap_or(value);
        if hex.is_empty() {
            return Ok(Self::zero());
        }
        BigUint::parse_bytes(hex.as_bytes(), 16)
            .map(Self)
            .ok_or(AmountError::Invalid)
    }

    pub fn from_decimal(value: &str, decimals: u8) -> Result<Self, AmountError> {
        if value.is_empty() || value.starts_with('-') || value.starts_with('+') {
            return Err(AmountError::Invalid);
        }
        let mut parts = value.split('.');
        let whole = parts.next().ok_or(AmountError::Invalid)?;
        let fraction = parts.next();
        if parts.next().is_some() || whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(AmountError::Invalid);
        }
        let fraction = fraction.unwrap_or("");
        if !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return Err(AmountError::Invalid);
        }
        if fraction.len() > usize::from(decimals) {
            return Err(AmountError::TooManyDecimals);
        }
        let mut atomic = whole.to_owned();
        atomic.push_str(fraction);
        atomic.extend(std::iter::repeat_n(
            '0',
            usize::from(decimals) - fraction.len(),
        ));
        let normalized = atomic.trim_start_matches('0');
        if normalized.is_empty() {
            return Ok(Self::zero());
        }
        normalized.parse()
    }

    #[must_use]
    pub fn to_decimal(&self, decimals: u8) -> String {
        if decimals == 0 {
            return self.to_string();
        }
        let mut digits = self.to_string();
        let decimals = usize::from(decimals);
        if digits.len() <= decimals {
            digits = format!("{}{}", "0".repeat(decimals + 1 - digits.len()), digits);
        }
        let split = digits.len() - decimals;
        let whole = &digits[..split];
        let fraction = digits[split..].trim_end_matches('0');
        if fraction.is_empty() {
            whole.to_owned()
        } else {
            format!("{whole}.{fraction}")
        }
    }

    #[must_use]
    pub fn checked_add(&self, rhs: &Self) -> Self {
        Self(&self.0 + &rhs.0)
    }

    #[must_use]
    pub fn checked_sub(&self, rhs: &Self) -> Option<Self> {
        (self.0 >= rhs.0).then(|| Self(&self.0 - &rhs.0))
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == BigUint::default()
    }
}

impl FromStr for AtomicAmount {
    type Err = AmountError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(AmountError::Invalid);
        }
        BigUint::from_str(value)
            .map(Self)
            .map_err(|_| AmountError::Invalid)
    }
}

impl fmt::Display for AtomicAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for AtomicAmount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AtomicAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_float_or_negative_atomic_amounts() {
        assert!("1.2".parse::<AtomicAmount>().is_err());
        assert!("-1".parse::<AtomicAmount>().is_err());
        assert!("".parse::<AtomicAmount>().is_err());
    }

    #[test]
    fn parses_human_decimal_without_floats() {
        assert_eq!(
            AtomicAmount::from_decimal("50", 6).unwrap().to_string(),
            "50000000"
        );
        assert_eq!(
            AtomicAmount::from_decimal("1.25", 6).unwrap().to_string(),
            "1250000"
        );
        assert!(AtomicAmount::from_decimal("1.0000001", 6).is_err());
    }

    #[test]
    fn formats_decimal() {
        let amount: AtomicAmount = "1250000".parse().unwrap();
        assert_eq!(amount.to_decimal(6), "1.25");
    }
}
