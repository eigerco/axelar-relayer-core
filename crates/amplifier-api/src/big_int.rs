//! `BigInt` type with configurable precision for blockchain-specific token amounts.

use core::fmt::Display;
use std::io::{Read, Result, Write};

#[allow(unused_imports, reason = "simplifies imports")]
use bnum::types::U256;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize};
use tracing::warn;

#[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
type InnerType = u64;
#[cfg(feature = "bigint-u128")]
type InnerType = u128;
#[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
type InnerType = bnum::types::U256;

/**
Represents a big integer as a string matching the pattern `^(0|[1-9]\d*)$`.
ref: <https://github.com/axelarnetwork/axelar-eds-mirror/blob/main/axelarons-gmp-api/schema/schema.yaml/>
The underlying type changes based on features:
- `bigint-u64`: uses u64
- `bigint-u128`: uses u128
- default: uses U256 (256-bit unsigned integer)

Note: Negative values are stored as None.
*/
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct BigInt(Option<InnerType>);

impl BigInt {
    /// Get the value, returning None if it's None (negative)
    #[must_use]
    pub const fn value(self) -> Option<InnerType> {
        self.0
    }

    /// Set the value
    pub const fn set_value(&mut self, value: InnerType) {
        self.0 = Some(value);
    }

    /// Check if the value is negative (None)
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0.is_none()
    }
}

impl From<InnerType> for BigInt {
    fn from(value: InnerType) -> Self {
        Self(Some(value))
    }
}

impl Display for BigInt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(value) => write!(f, "{value}"),
            None => {
                write!(f, "Negative")
            }
        }
    }
}

impl Serialize for BigInt {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(value) = self.0 {
            let string = value.to_string();
            serializer.serialize_str(&string)
        } else {
            warn!("Serializing negative BigInt value as '0'");
            serializer.serialize_str("0")
        }
    }
}

impl<'de> Deserialize<'de> for BigInt {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = <String as serde::Deserialize>::deserialize(deserializer)?;

        // Check if the string starts with a negative sign
        if string.starts_with('-') {
            warn!(
                "Attempted to deserialize negative value '{}' into BigInt, storing as None",
                string
            );
            return Ok(Self(None));
        }

        #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
        let number = string
            .parse::<u64>()
            .map_err(|err| serde::de::Error::custom(format!("Failed to parse u64, err: {err}")))?;
        #[cfg(feature = "bigint-u128")]
        let number = string
            .parse::<u128>()
            .map_err(|err| serde::de::Error::custom(format!("Failed to parse u128, err: {err}")))?;
        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        let number = U256::from_str_radix(&string, 10)
            .map_err(|err| serde::de::Error::custom(format!("Failed to parse U256, err: {err}")))?;

        Ok(Self(Some(number)))
    }
}

/// Serialize `BigInt` for Borsh
///
/// # Errors
/// Infallible
#[allow(clippy::trivially_copy_pass_by_ref, reason = "needs by ref")]
pub fn serialize<W: Write>(value: &BigInt, writer: &mut W) -> Result<()> {
    match value.0 {
        Some(inner_value) => {
            <String as BorshSerialize>::serialize(&inner_value.to_string(), writer)
        }
        None => <String as BorshSerialize>::serialize(&"0".to_owned(), writer),
    }
}

/// Deserialize `BigInt` for Borsh
///
/// # Errors
/// wrong input
pub fn deserialize<R: Read>(reader: &mut R) -> Result<BigInt> {
    let value: String = BorshDeserialize::deserialize_reader(reader)?;

    // Check if the string starts with a negative sign
    if value.starts_with('-') {
        warn!(
            "Attempted to deserialize negative value '{}' into BigInt, storing as None",
            value
        );
        return Ok(BigInt(None));
    }

    #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
    let number = value
        .parse::<u64>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    #[cfg(feature = "bigint-u128")]
    let number = value
        .parse::<u128>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
    let number = U256::from_str_radix(&value, 10).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse U256, err: {err}"),
        )
    })?;
    Ok(BigInt(Some(number)))
}

#[cfg(test)]
mod tests {
    #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
    use bnum::types::U256;

    use super::*;

    #[derive(BorshSerialize, BorshDeserialize)]
    struct BigIntContainer {
        #[borsh(serialize_with = "serialize", deserialize_with = "deserialize")]
        pub value: BigInt,
    }

    #[test]
    fn test_bigint_creation() {
        #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
        let bigint: BigInt = BigInt::from(42_u64);
        #[cfg(feature = "bigint-u128")]
        let bigint: BigInt = BigInt::from(42_u128);
        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        let bigint: BigInt = BigInt::from(U256::from(42_u64));
        assert_eq!(bigint.to_string(), "42");
    }

    #[test]
    fn test_bigint_serialization() {
        #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
        let bigint: BigInt = BigInt::from(12345_u64);
        #[cfg(feature = "bigint-u128")]
        let bigint: BigInt = BigInt::from(12345_u128);
        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        let bigint: BigInt = BigInt::from(U256::from(12345_u64));
        let serialized = serde_json::to_string(&bigint).unwrap();
        assert_eq!(serialized, "\"12345\"");
    }

    #[test]
    fn test_bigint_deserialization() {
        let json = "\"98765\"";
        let bigint: BigInt = serde_json::from_str(json).unwrap();
        assert_eq!(bigint.to_string(), "98765");
    }

    #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
    #[test]
    fn test_borsh_serialization_u64() {
        let bigint: BigInt = 999_u64.into();
        let mut buffer = Vec::new();
        serialize(&bigint, &mut buffer).unwrap();

        let deserialized = deserialize(&mut buffer.as_slice()).unwrap();
        assert_eq!(bigint, deserialized);

        // Test with u64::MAX
        let max_bigint: BigInt = u64::MAX.into();
        let mut max_buffer = Vec::new();
        serialize(&max_bigint, &mut max_buffer).unwrap();

        let max_deserialized = deserialize(&mut max_buffer.as_slice()).unwrap();
        assert_eq!(max_bigint, max_deserialized);
    }

    #[cfg(feature = "bigint-u128")]
    #[test]
    fn test_borsh_serialization_u128() {
        let bigint: BigInt = 999_u128.into();
        let mut buffer = Vec::new();
        serialize(&bigint, &mut buffer).unwrap();

        let deserialized = deserialize(&mut buffer.as_slice()).unwrap();
        assert_eq!(bigint, deserialized);

        // Test with u128::MAX
        let max_bigint: BigInt = u128::MAX.into();
        let mut max_buffer = Vec::new();
        serialize(&max_bigint, &mut max_buffer).unwrap();

        let max_deserialized = deserialize(&mut max_buffer.as_slice()).unwrap();
        assert_eq!(max_bigint, max_deserialized);
    }

    #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
    #[test]
    fn test_borsh_serialization_u256() {
        let bigint: BigInt = U256::from(999_u64).into();
        let mut buffer = Vec::new();
        serialize(&bigint, &mut buffer).unwrap();

        let deserialized = deserialize(&mut buffer.as_slice()).unwrap();
        assert_eq!(bigint, deserialized);

        // Test with a value larger than u128::MAX
        let large_value =
            U256::from_str_radix("340282366920938463463374607431768211456", 10).unwrap();
        let large_bigint: BigInt = large_value.into();
        let mut large_buffer = Vec::new();
        serialize(&large_bigint, &mut large_buffer).unwrap();

        let large_deserialized = deserialize(&mut large_buffer.as_slice()).unwrap();
        assert_eq!(large_bigint, large_deserialized);
    }

    #[test]
    fn test_large_numbers() {
        // Test with a number that fits in u64
        let json = "\"18446744073709551615\""; // u64::MAX
        let bigint: BigInt = serde_json::from_str(json).unwrap();
        assert_eq!(bigint.to_string(), "18446744073709551615");

        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        {
            // Test with a number larger than u128 (only for U256)
            let large_json = "\"340282366920938463463374607431768211456\""; // u128::MAX + 1
            let large_bigint: BigInt = serde_json::from_str(large_json).unwrap();
            assert_eq!(
                large_bigint.to_string(),
                "340282366920938463463374607431768211456"
            );
        }
    }

    #[test]
    fn test_zero() {
        #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
        let bigint: BigInt = 0_u64.into();
        #[cfg(feature = "bigint-u128")]
        let bigint: BigInt = 0_u128.into();
        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        let bigint: BigInt = U256::from(0_u64).into();
        assert_eq!(bigint.to_string(), "0");

        let serialized = serde_json::to_string(&bigint).unwrap();
        assert_eq!(serialized, "\"0\"");
    }

    #[test]
    fn test_negative_values_treated_as_zero() {
        // Test JSON deserialization with negative value
        let negative_json = "\"-12345\"";
        let bigint: BigInt = serde_json::from_str(negative_json).unwrap();
        assert_eq!(bigint.to_string(), "Negative");

        // Test Borsh deserialization with negative value
        let negative_string = "-98765".to_string();
        let mut buffer = Vec::new();
        <String as BorshSerialize>::serialize(&negative_string, &mut buffer).unwrap();
        let deserialized = deserialize(&mut buffer.as_slice()).unwrap();
        assert_eq!(deserialized.to_string(), "Negative");
    }

    #[test]
    fn test_negative_values_stored_as_none() {
        // Test JSON deserialization with negative value
        let negative_json = "\"-12345\"";
        let bigint: BigInt = serde_json::from_str(negative_json).unwrap();

        // Verify internal representation is None
        assert_eq!(bigint.0, None);

        // Verify serialization outputs "0"
        let serialized = serde_json::to_string(&bigint).unwrap();
        assert_eq!(serialized, "\"0\"");

        // Verify display shows "Negative"
        assert_eq!(bigint.to_string(), "Negative");
    }

    #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
    #[test]
    fn test_u64_specific() {
        let max: BigInt = u64::MAX.into();
        assert_eq!(max.0, Some(u64::MAX));
    }

    #[cfg(feature = "bigint-u128")]
    #[test]
    fn test_u128_specific() {
        let max: BigInt = u128::MAX.into();
        assert_eq!(max.0, Some(u128::MAX));
    }

    #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
    #[test]
    fn test_u256_specific() {
        let max: BigInt = U256::MAX.into();
        assert_eq!(max.0, Some(U256::MAX));
    }

    #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
    #[test]
    fn test_bigint_borsh_serialize_and_deserialize_u64() {
        let value = u64::MAX;
        let container = BigIntContainer {
            value: BigInt::from(value),
        };
        let serialized = borsh::to_vec(&container).expect("serialize bigint succeeds");
        let deserialized =
            BigIntContainer::deserialize(&mut serialized.as_slice()).expect("deserize suceeds");

        assert_eq!(BigInt::from(value), deserialized.value);
    }

    #[cfg(feature = "bigint-u128")]
    #[test]
    fn test_bigint_borsh_serialize_and_deserialize_u128() {
        let value = u128::MAX;
        let container = BigIntContainer {
            value: BigInt::from(value),
        };
        let serialized = borsh::to_vec(&container).expect("serialize bigint succeeds");
        let deserialized =
            BigIntContainer::deserialize(&mut serialized.as_slice()).expect("deserize suceeds");

        assert_eq!(BigInt::from(value), deserialized.value);
    }

    #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
    #[test]
    fn test_bigint_borsh_serialize_and_deserialize_u256() {
        let value = U256::from_str_radix("423423413123813194728478923748923748923749872984732", 10)
            .unwrap();
        let container = BigIntContainer {
            value: value.into(),
        };
        let serialized = borsh::to_vec(&container).expect("serialize bigint succeeds");
        let deserialized =
            BigIntContainer::deserialize(&mut serialized.as_slice()).expect("deserize suceeds");

        assert_eq!(Some(value), deserialized.value.value());
    }

    #[test]
    fn test_getter_setter() {
        #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
        let mut bigint = BigInt::from(100_u64);
        #[cfg(feature = "bigint-u128")]
        let mut bigint = BigInt::from(100_u128);
        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        let mut bigint = BigInt::from(U256::from(100_u64));

        // Test getter
        #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
        assert_eq!(bigint.value(), Some(100_u64));
        #[cfg(feature = "bigint-u128")]
        assert_eq!(bigint.value(), Some(100_u128));
        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        assert_eq!(bigint.value(), Some(U256::from(100_u64)));

        // Test is_negative for positive value
        assert!(!bigint.is_negative());

        // Test setter
        #[cfg(all(feature = "bigint-u64", not(feature = "bigint-u128")))]
        bigint.set_value(200_u64);
        #[cfg(feature = "bigint-u128")]
        bigint.set_value(200_u128);
        #[cfg(all(not(feature = "bigint-u64"), not(feature = "bigint-u128")))]
        bigint.set_value(U256::from(200_u64));

        assert_eq!(bigint.to_string(), "200");
        assert!(!bigint.is_negative());

        // Test that negative values return None via getter and true for is_negative
        let negative_json = "\"-123\"";
        let negative_bigint: BigInt = serde_json::from_str(negative_json).unwrap();

        assert_eq!(negative_bigint.value(), None);
        assert!(negative_bigint.is_negative());
    }
}
