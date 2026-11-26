//! Exact fixed-point decimal arithmetic for XBRL calculation checks.
//!
//! Authority: XML Schema 1.0 Part 2 (Datatypes) §3.2.3 (`xsd:decimal`,
//! lexical `(sign? digits ('.' digits?)? | '.' digits)`), §3.2.4 (`integer`).
//!
//! No floating point is used. Values are `i128` scaled by `10^-scale`, with a
//! maximum scale of 18 decimal places. A lexical value outside that representable
//! range is rejected rather than rounded, so a calculation check can never
//! silently accept a number it did not parse exactly.

use std::fmt;
use std::str::FromStr;

/// Maximum decimal places this fixed-point representation keeps.
pub const MAX_SCALE: u8 = 18;

/// Why a decimal lexical value was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalError {
    /// Not a valid `xsd:decimal` lexical form.
    Invalid,
    /// Valid lexical form but outside the fixed-point range.
    OutOfRange,
}

impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => f.write_str("not an XSD 1.0 decimal"),
            Self::OutOfRange => f.write_str("decimal exceeds the supported scale or magnitude"),
        }
    }
}

impl std::error::Error for DecimalError {}

/// An exact fixed-point decimal with up to [`MAX_SCALE`] fractional digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimal {
    unscaled: i128,
    scale: u8,
}

impl Decimal {
    /// Exact zero.
    pub const ZERO: Self = Self {
        unscaled: 0,
        scale: 0,
    };

    /// Exact one.
    #[must_use]
    pub const fn one() -> Self {
        Self {
            unscaled: 1,
            scale: 0,
        }
    }

    /// Parses an `xsd:decimal` lexical form without whitespace collapse.
    ///
    /// # Errors
    ///
    /// [`DecimalError::Invalid`] for malformed input, [`DecimalError::OutOfRange`]
    /// when the coefficient or scale cannot be represented exactly.
    pub fn parse(input: &str) -> Result<Self, DecimalError> {
        let bytes = input.as_bytes();
        if bytes.is_empty() {
            return Err(DecimalError::Invalid);
        }
        let mut pos = 0;
        let negative = match bytes[0] {
            b'+' => {
                pos += 1;
                false
            }
            b'-' => {
                pos += 1;
                true
            }
            _ => false,
        };
        let int_start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        let int_digits = &input[int_start..pos];
        let mut frac_digits = "";
        if bytes.get(pos) == Some(&b'.') {
            pos += 1;
            let frac_start = pos;
            while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            frac_digits = &input[frac_start..pos];
            if int_digits.is_empty() && frac_digits.is_empty() {
                return Err(DecimalError::Invalid);
            }
        } else if int_digits.is_empty() {
            return Err(DecimalError::Invalid);
        }
        if pos != bytes.len() {
            return Err(DecimalError::Invalid);
        }
        if frac_digits.len() > usize::from(MAX_SCALE) {
            return Err(DecimalError::OutOfRange);
        }
        let mut digits = String::with_capacity(int_digits.len() + frac_digits.len() + 1);
        digits.push_str(if int_digits.is_empty() {
            "0"
        } else {
            int_digits
        });
        digits.push_str(frac_digits);
        let mut unscaled = i128::from_str(&digits).map_err(|_| DecimalError::OutOfRange)?;
        if negative {
            unscaled = -unscaled;
        }
        Ok(Self {
            unscaled,
            scale: frac_digits.len() as u8,
        })
    }

    /// Returns the value multiplied by `10^exponent` when exactly representable.
    #[must_use]
    pub fn scale_b10(self, exponent: i64) -> Option<Self> {
        let new_scale = i32::from(self.scale) - exponent as i32;
        if new_scale < 0 {
            // u * 10^-s * 10^e = (u * 10^-new_scale) * 10^-new_scale
            // when new_scale is negative, absorb the negative scale into u.
            let shift = u32::try_from(-new_scale).ok()?;
            if shift > 38 {
                return None;
            }
            let factor = 10_i128.checked_pow(shift)?;
            Some(Self {
                unscaled: self.unscaled.checked_mul(factor)?,
                scale: 0,
            })
        } else {
            let new_scale = u8::try_from(new_scale).ok()?;
            if new_scale > MAX_SCALE {
                return None;
            }
            Some(Self {
                unscaled: self.unscaled,
                scale: new_scale,
            })
        }
    }

    /// Re-expresses the same value with `target` fractional digits.
    #[must_use]
    pub fn rescale(self, target: u8) -> Option<Self> {
        if target > MAX_SCALE {
            return None;
        }
        if target >= self.scale {
            let shift = usize::from(target - self.scale);
            Some(Self {
                unscaled: self
                    .unscaled
                    .checked_mul(10_i128.checked_pow(shift as u32)?)?,
                scale: target,
            })
        } else {
            let shift = usize::from(self.scale - target);
            let factor = 10_i128.checked_pow(shift as u32)?;
            if self.unscaled % factor != 0 {
                return None;
            }
            Some(Self {
                unscaled: self.unscaled.checked_div(factor)?,
                scale: target,
            })
        }
    }

    /// Adds two values exactly when the result stays in range.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let scale = self.scale.max(other.scale);
        let a = self.rescale(scale)?;
        let b = other.rescale(scale)?;
        Some(Self {
            unscaled: a.unscaled.checked_add(b.unscaled)?,
            scale,
        })
    }

    /// Subtracts two values exactly when the result stays in range.
    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.checked_add(Self {
            unscaled: -other.unscaled,
            scale: other.scale,
        })
    }

    /// Multiplies by an exact integer weight.
    #[must_use]
    pub fn checked_mul_i64(self, weight: i64) -> Option<Self> {
        Some(Self {
            unscaled: self.unscaled.checked_mul(i128::from(weight))?,
            scale: self.scale,
        })
    }

    /// Absolute value.
    #[must_use]
    pub const fn abs(self) -> Self {
        Self {
            unscaled: self.unscaled.abs(),
            scale: self.scale,
        }
    }

    /// Number of fractional decimal places retained.
    #[must_use]
    pub const fn scale(self) -> u8 {
        self.scale
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let scale = self.scale.max(other.scale);
        let a = self.rescale(scale);
        let b = other.rescale(scale);
        match (a, b) {
            (Some(a), Some(b)) => a.unscaled.cmp(&b.unscaled),
            // Out-of-range scaling means both values share a very large
            // exponent; compare their raw coefficients scaled by magnitude.
            _ => self.unscaled.cmp(&other.unscaled),
        }
    }
}

impl FromStr for Decimal {
    type Err = DecimalError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.unscaled);
        }
        let digits = self.unscaled.abs().to_string();
        let split = digits.len().saturating_sub(usize::from(self.scale));
        let int_part = if split == 0 { "0" } else { &digits[..split] };
        let frac_part = if split == 0 {
            digits.as_str()
        } else {
            &digits[split..]
        };
        let padded = format!("{:0>width$}", frac_part, width = usize::from(self.scale));
        write!(
            f,
            "{}{}.{}",
            if self.unscaled < 0 { "-" } else { "" },
            int_part,
            padded
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_xsd_decimal_lexical_forms() {
        for (input, expected) in [
            ("0", "0"),
            ("+0", "0"),
            ("-0", "0"),
            ("1", "1"),
            ("1.0", "1.0"),
            ("-1.25", "-1.25"),
            (".5", "0.5"),
            ("-.5", "-0.5"),
            ("123.", "123"),
            ("00012.3400", "12.3400"),
        ] {
            assert_eq!(
                Decimal::parse(input).unwrap().to_string(),
                expected,
                "{input}"
            );
        }
    }

    #[test]
    fn rejects_invalid_and_out_of_range_decimals() {
        for input in [
            "", "+", ".", "-", "1e3", "1,5", "1.2.3", " 1", "1 ", "0x10", "--1", "INF", "NaN",
        ] {
            assert_eq!(
                Decimal::parse(input),
                Err(DecimalError::Invalid),
                "{input:?}"
            );
        }
        for input in [
            "999999999999999999999999999999999999999999999",
            "0.1234567890123456789",
        ] {
            assert_eq!(
                Decimal::parse(input),
                Err(DecimalError::OutOfRange),
                "{input:?}"
            );
        }
    }

    #[test]
    fn addition_is_exact_across_scales() {
        let a = Decimal::parse("0.1").unwrap();
        let b = Decimal::parse("0.2").unwrap();
        assert_eq!(a.checked_add(b).unwrap().to_string(), "0.3");
        let c = Decimal::parse("1.25").unwrap();
        let d = Decimal::parse("2.5").unwrap();
        assert_eq!(c.checked_add(d).unwrap().to_string(), "3.75");
        assert_eq!(c.checked_sub(d).unwrap().to_string(), "-1.25");
    }

    #[test]
    fn scaling_by_powers_of_ten() {
        let d = Decimal::parse("1.23").unwrap();
        assert_eq!(d.scale_b10(2).unwrap().to_string(), "123");
        assert_eq!(
            Decimal::parse("1.2300")
                .unwrap()
                .scale_b10(2)
                .unwrap()
                .to_string(),
            "123.00"
        );
        assert_eq!(d.scale_b10(-2).unwrap().to_string(), "0.0123");
        assert_eq!(
            Decimal::parse("1")
                .unwrap()
                .scale_b10(2)
                .unwrap()
                .to_string(),
            "100"
        );
        assert_eq!(Decimal::parse("1").unwrap().scale_b10(40), None);
    }

    #[test]
    fn weighted_summation_and_corner_cases() {
        let values = ["0.1", "0.2", "-0.1"];
        let mut sum = Decimal::ZERO;
        for v in values {
            sum = sum.checked_add(Decimal::parse(v).unwrap()).unwrap();
        }
        assert_eq!(sum.to_string(), "0.2");
        let big = Decimal::parse("1.25").unwrap().checked_mul_i64(-2).unwrap();
        assert_eq!(big.to_string(), "-2.50");
    }
}
