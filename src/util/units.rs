//! Human-friendly byte sizes (binary units).

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// A size in bytes.
///
/// Parses whole numbers with an optional binary suffix (`K`, `M`, `G`, `T`,
/// optionally followed by `iB` or `B`, case-insensitive).
///
/// ```
/// use multithreaded_data_transfer_rs::ByteSize;
///
/// let size: ByteSize = "8M".parse()?;
/// assert_eq!(size, ByteSize::mib(8));
/// assert_eq!(size.to_string(), "8 MiB");
/// # Ok::<(), multithreaded_data_transfer_rs::util::units::ParseByteSizeError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ByteSize(u64);

impl ByteSize {
    /// One kibibyte.
    pub const KIB: Self = Self(1 << 10);
    /// One mebibyte.
    pub const MIB: Self = Self(1 << 20);
    /// One gibibyte.
    pub const GIB: Self = Self(1 << 30);
    /// One tebibyte.
    pub const TIB: Self = Self(1 << 40);

    const UNITS: [(Self, &'static str); 4] = [
        (Self::TIB, "TiB"),
        (Self::GIB, "GiB"),
        (Self::MIB, "MiB"),
        (Self::KIB, "KiB"),
    ];

    /// Wraps a raw byte count.
    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    /// `n` kibibytes (saturating).
    pub const fn kib(n: u64) -> Self {
        Self(n.saturating_mul(Self::KIB.0))
    }

    /// `n` mebibytes (saturating).
    pub const fn mib(n: u64) -> Self {
        Self(n.saturating_mul(Self::MIB.0))
    }

    /// `n` gibibytes (saturating).
    pub const fn gib(n: u64) -> Self {
        Self(n.saturating_mul(Self::GIB.0))
    }

    /// The raw byte count.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The byte count as `usize`, saturating on 32-bit targets.
    pub fn as_usize(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

impl From<u64> for ByteSize {
    fn from(bytes: u64) -> Self {
        Self(bytes)
    }
}

impl From<ByteSize> for u64 {
    fn from(size: ByteSize) -> Self {
        size.0
    }
}

/// Why a string could not be parsed as a [`ByteSize`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseByteSizeError {
    /// The input was empty or whitespace.
    #[error("size is empty")]
    Empty,
    /// The numeric part was missing or not a whole number.
    #[error("invalid number `{0}`: use a whole number like 512K or 8M")]
    InvalidNumber(String),
    /// The suffix is not a known unit.
    #[error("unknown suffix `{0}` (use K, M, G or T)")]
    UnknownSuffix(String),
    /// The value does not fit in 64 bits.
    #[error("size is too large")]
    Overflow,
}

impl FromStr for ByteSize {
    type Err = ParseByteSizeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ParseByteSizeError::Empty);
        }

        let digits_end = input
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(input.len());
        let (number, suffix) = input.split_at(digits_end);
        if number.is_empty() || suffix.starts_with(['.', ',']) {
            return Err(ParseByteSizeError::InvalidNumber(input.to_owned()));
        }

        // Only ASCII digits remain, so the only possible failure is overflow.
        let value: u64 = number.parse().map_err(|_| ParseByteSizeError::Overflow)?;
        let suffix = suffix.trim();
        let unit = match suffix.to_ascii_lowercase().as_str() {
            "" | "b" => 1,
            "k" | "kb" | "kib" => Self::KIB.0,
            "m" | "mb" | "mib" => Self::MIB.0,
            "g" | "gb" | "gib" => Self::GIB.0,
            "t" | "tb" | "tib" => Self::TIB.0,
            _ => return Err(ParseByteSizeError::UnknownSuffix(suffix.to_owned())),
        };

        value
            .checked_mul(unit)
            .map(Self)
            .ok_or(ParseByteSizeError::Overflow)
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(&(unit, name)) = Self::UNITS.iter().find(|(unit, _)| self.0 >= unit.0) else {
            return write!(f, "{} B", self.0);
        };
        if self.0.is_multiple_of(unit.0) {
            write!(f, "{} {name}", self.0 / unit.0)
        } else {
            #[expect(clippy::cast_precision_loss, reason = "display rounding only")]
            let scaled = self.0 as f64 / unit.0 as f64;
            write!(f, "{scaled:.2} {name}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    mod parsing {
        use super::*;

        #[test]
        fn parses_plain_bytes() {
            assert_eq!("4096".parse(), Ok(ByteSize::new(4096)));
        }

        #[test]
        fn parses_kib_suffix_case_insensitive() {
            assert_eq!("512k".parse(), Ok(ByteSize::kib(512)));
        }

        #[test]
        fn parses_mib_with_ib_suffix() {
            assert_eq!("8MiB".parse(), Ok(ByteSize::mib(8)));
        }

        #[test]
        fn parses_space_between_number_and_unit() {
            assert_eq!("2 G".parse(), Ok(ByteSize::gib(2)));
        }

        #[test]
        fn rejects_empty() {
            assert_eq!("  ".parse::<ByteSize>(), Err(ParseByteSizeError::Empty));
        }

        #[test]
        fn rejects_unknown_suffix() {
            assert_eq!(
                "12X".parse::<ByteSize>(),
                Err(ParseByteSizeError::UnknownSuffix("X".into()))
            );
        }

        #[test]
        fn rejects_fractions() {
            assert!(matches!(
                "1.5G".parse::<ByteSize>(),
                Err(ParseByteSizeError::InvalidNumber(_))
            ));
        }

        #[test]
        fn rejects_missing_number() {
            assert!(matches!(
                "M".parse::<ByteSize>(),
                Err(ParseByteSizeError::InvalidNumber(_))
            ));
        }

        #[test]
        fn rejects_overflow() {
            assert_eq!(
                "99999999999T".parse::<ByteSize>(),
                Err(ParseByteSizeError::Overflow)
            );
        }
    }

    mod display {
        use super::*;

        #[test]
        fn shows_small_values_in_bytes() {
            assert_eq!(ByteSize::new(512).to_string(), "512 B");
        }

        #[test]
        fn shows_whole_units_without_decimals() {
            assert_eq!(ByteSize::gib(1).to_string(), "1 GiB");
        }

        #[test]
        fn shows_fractional_units_with_two_decimals() {
            assert_eq!(ByteSize::new(1536).to_string(), "1.50 KiB");
        }
    }

    proptest! {
        #[test]
        fn display_then_parse_roundtrips_for_whole_units(
            n in 1u64..1024,
            unit in prop::sample::select(vec![1, 1 << 10, 1 << 20, 1 << 30]),
        ) {
            let size = ByteSize::new(n * unit);
            prop_assert_eq!(size.to_string().parse::<ByteSize>(), Ok(size));
        }

        #[test]
        fn parsing_never_panics(input in ".{0,24}") {
            let _ = input.parse::<ByteSize>();
        }
    }
}
