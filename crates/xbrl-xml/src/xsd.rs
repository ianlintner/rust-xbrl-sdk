//! Strict lexical handling for the XML Schema 1.0 datatypes XBRL 2.1 uses.
//!
//! Authority: XML Schema Part 2 (Datatypes, 1.0 2nd ed.) §3.2.7 `dateTime`,
//! §3.2.9 `date`, §3.2.19 whitespace facet `collapse`; XBRL 2.1 §4.7.2 for the
//! `xbrli:dateUnion` interpretation (a date-only `endDate`/`instant` means the
//! start of the following day, and `24:00:00` is not used).
//!
//! Supported lexical space: positive years of four to nine digits (no leading
//! zeros beyond four digits, year `0000` rejected as in XSD 1.0), `T` and `Z`
//! upper-case only, seconds `00`–`59`, optional fractional seconds, and a
//! timezone of `Z` or `±hh:mm` with `hh:mm` at most `14:00`. Negative years and
//! years of more than nine digits are reported as [`TemporalError::Unsupported`]
//! rather than guessed at.

/// Why a `date`/`dateTime` lexical form was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemporalError {
    /// Not in the XSD `date` or `dateTime` lexical space (or not a real date).
    Invalid,
    /// Lexically plausible but outside the range this crate represents.
    Unsupported,
}

/// A point on the (partial) XSD time line, normalized for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Temporal {
    /// Seconds since 0001-01-01-like epoch of the proleptic Gregorian
    /// calendar. UTC seconds when `zoned`, local seconds otherwise.
    seconds: i64,
    /// Fractional seconds digits with trailing zeros removed (may be empty).
    fraction: String,
    zoned: bool,
    /// The lexical form carried no time part.
    date_only: bool,
}

const MAX_OFFSET_SECONDS: i64 = 14 * 3600;

/// XML 1.0 whitespace: exactly #x20, #x9, #xA, #xD (not Unicode spaces).
pub(crate) const fn is_xml_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// XSD `whitespace=collapse`: replace XML whitespace runs by one space and
/// trim the ends. Non-XML spaces such as U+00A0 are ordinary characters.
pub(crate) fn collapse(s: &str) -> String {
    s.split(is_xml_whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when `s` consists only of XML whitespace (or is empty).
pub(crate) fn is_blank(s: &str) -> bool {
    s.chars().all(is_xml_whitespace)
}

fn digits(bytes: &[u8], pos: &mut usize, n: usize) -> Option<i64> {
    let slice = bytes.get(*pos..*pos + n)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    *pos += n;
    Some(
        slice
            .iter()
            .fold(0, |acc, b| acc * 10 + i64::from(b - b'0')),
    )
}

fn expect(bytes: &[u8], pos: &mut usize, byte: u8) -> Option<()> {
    if bytes.get(*pos) == Some(&byte) {
        *pos += 1;
        Some(())
    } else {
        None
    }
}

const fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(year) => 29,
        _ => 28,
    }
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (H. Hinnant).
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Temporal {
    /// Parses an `xsd:date` or `xsd:dateTime` lexical form (already
    /// whitespace-collapsed by the caller).
    pub(crate) fn parse(s: &str) -> Result<Self, TemporalError> {
        let bytes = s.as_bytes();
        if bytes.first() == Some(&b'-') && bytes.get(1).is_some_and(u8::is_ascii_digit) {
            return Err(TemporalError::Unsupported);
        }
        let year_len = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        if year_len < 4 {
            return Err(TemporalError::Invalid);
        }
        if year_len > 9 {
            return Err(TemporalError::Unsupported);
        }
        if year_len > 4 && bytes[0] == b'0' {
            return Err(TemporalError::Invalid);
        }
        let mut pos = 0;
        let year = digits(bytes, &mut pos, year_len).ok_or(TemporalError::Invalid)?;
        Self::parse_rest(bytes, pos, year).ok_or(TemporalError::Invalid)
    }

    fn parse_rest(bytes: &[u8], mut pos: usize, year: i64) -> Option<Self> {
        if year == 0 {
            return None;
        }
        expect(bytes, &mut pos, b'-')?;
        let month = digits(bytes, &mut pos, 2)?;
        expect(bytes, &mut pos, b'-')?;
        let day = digits(bytes, &mut pos, 2)?;
        if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        let mut time_seconds = 0;
        let mut fraction = String::new();
        let date_only = bytes.get(pos) != Some(&b'T');
        if !date_only {
            pos += 1;
            let hour = digits(bytes, &mut pos, 2)?;
            expect(bytes, &mut pos, b':')?;
            let minute = digits(bytes, &mut pos, 2)?;
            expect(bytes, &mut pos, b':')?;
            let second = digits(bytes, &mut pos, 2)?;
            if hour > 23 || minute > 59 || second > 59 {
                return None;
            }
            time_seconds = hour * 3600 + minute * 60 + second;
            if bytes.get(pos) == Some(&b'.') {
                pos += 1;
                let start = pos;
                while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
                    pos += 1;
                }
                if pos == start {
                    return None;
                }
                fraction = String::from_utf8_lossy(&bytes[start..pos])
                    .trim_end_matches('0')
                    .to_owned();
            }
        }
        let offset = match bytes.get(pos) {
            None => None,
            Some(b'Z') => {
                pos += 1;
                Some(0)
            }
            Some(sign @ (b'+' | b'-')) => {
                pos += 1;
                let hours = digits(bytes, &mut pos, 2)?;
                expect(bytes, &mut pos, b':')?;
                let minutes = digits(bytes, &mut pos, 2)?;
                let total = hours * 3600 + minutes * 60;
                if minutes > 59 || total > MAX_OFFSET_SECONDS {
                    return None;
                }
                Some(if *sign == b'-' { -total } else { total })
            }
            Some(_) => return None,
        };
        if pos != bytes.len() {
            return None;
        }
        let local = days_from_civil(year, month, day) * 86_400 + time_seconds;
        Some(Self {
            seconds: offset.map_or(local, |o| local - o),
            fraction,
            zoned: offset.is_some(),
            date_only,
        })
    }

    /// Applies XBRL 2.1 §4.7.2: a date-only `endDate`/`instant` is midnight at
    /// the *end* of that day, i.e. start of the next day.
    fn as_end_boundary(&self) -> Self {
        let mut end = self.clone();
        if self.date_only {
            end.seconds += 86_400;
        }
        end
    }
}

/// True when the duration is *certainly* not forward in time, using
/// XSD 1.0's partial order. A zoned and a zone-less value are ordered only
/// when they differ by more than the maximum ±14:00 offset; otherwise this
/// returns `false` (indeterminate is not reported as an error).
pub(crate) fn duration_is_reversed(start: &Temporal, end: &Temporal) -> bool {
    let end = end.as_end_boundary();
    let key = |t: &Temporal, shift: i64| (t.seconds + shift, t.fraction.clone());
    match (start.zoned, end.zoned) {
        (true, true) | (false, false) => key(&end, 0) <= key(start, 0),
        // Zone-less start may lie up to 14h before the lexical time in UTC.
        (false, true) => key(&end, 0) <= key(start, -MAX_OFFSET_SECONDS),
        // Zone-less end may lie up to 14h after the lexical time in UTC.
        (true, false) => key(&end, MAX_OFFSET_SECONDS) <= key(start, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> Temporal {
        Temporal::parse(s).unwrap_or_else(|e| panic!("{s}: {e:?}"))
    }

    #[test]
    fn accepts_xsd_lexical_forms() {
        for s in [
            "2025-01-05",
            "2025-01-05Z",
            "2025-01-05+05:30",
            "2025-01-05-14:00",
            "2025-01-05+14:00",
            "2024-02-29",
            "2000-02-29",
            "12025-01-05",
            "2025-01-05T00:00:00",
            "2025-01-05T23:59:59Z",
            "2025-01-05T10:00:00.123-08:00",
        ] {
            assert!(Temporal::parse(s).is_ok(), "{s}");
        }
    }

    #[test]
    fn rejects_non_xsd_lexical_forms() {
        for s in [
            "",
            "2025-1-5",
            "+2025-01-05",
            "25-01-05",
            "025-01-05",
            "02025-01-05",
            "0000-01-01",
            "2025-01-05t10:00:00",
            "2025-01-05z",
            "2025-01-05T10:00:60",
            "2025-01-05T24:00:00",
            "2025-01-05T10:00",
            "2025-01-05T10:00:00.",
            "2025-01-05+15:00",
            "2025-01-05+14:01",
            "2025-01-05+05:60",
            "2025-01-05+0530",
            "2025-13-05",
            "2025-00-05",
            "2025-01-00",
            "2025-02-29",
            "1900-02-29",
            "2025-04-31",
            "2025-01-05x",
            "2025-01-05\u{a0}",
            "2025-01-05 ",
            "２０２５-01-05",
        ] {
            assert_eq!(Temporal::parse(s), Err(TemporalError::Invalid), "{s:?}");
        }
    }

    #[test]
    fn unrepresentable_years_are_unsupported_not_invalid() {
        for s in ["-0001-01-01", "-12025-06-01", "1234567890-01-01"] {
            assert_eq!(Temporal::parse(s), Err(TemporalError::Unsupported), "{s}");
        }
    }

    #[test]
    fn collapse_uses_only_xml_whitespace() {
        assert_eq!(collapse(" \t a \r\n b  "), "a b");
        assert_eq!(collapse("\u{a0}a\u{a0}"), "\u{a0}a\u{a0}");
        assert!(is_blank(" \t\r\n"));
        assert!(!is_blank("\u{a0}"));
    }

    #[test]
    fn date_only_end_is_end_of_day() {
        // Same calendar day is a valid one-day duration.
        assert!(!duration_is_reversed(&ok("2025-01-05"), &ok("2025-01-05")));
        // Day before is reversed, and so is a date end equal to the start.
        assert!(duration_is_reversed(&ok("2025-02-01"), &ok("2025-01-31")));
        assert!(duration_is_reversed(&ok("2025-01-06Z"), &ok("2025-01-05Z")));
        assert!(duration_is_reversed(
            &ok("2025-01-05"),
            &ok("2025-01-04T00:00:00")
        ));
    }

    #[test]
    fn datetime_durations_must_move_forward() {
        assert!(duration_is_reversed(
            &ok("2025-01-05T10:00:00Z"),
            &ok("2025-01-05T10:00:00Z")
        ));
        assert!(!duration_is_reversed(
            &ok("2025-01-05T10:00:00Z"),
            &ok("2025-01-05T10:00:01Z")
        ));
        // Offsets are normalized: 10:00-05:00 is 15:00Z.
        assert!(duration_is_reversed(
            &ok("2025-01-05T10:00:00-05:00"),
            &ok("2025-01-05T14:00:00Z")
        ));
        // Fractions compare numerically, not lexically.
        assert!(!duration_is_reversed(
            &ok("2025-01-05T10:00:00.5"),
            &ok("2025-01-05T10:00:00.50001")
        ));
        assert!(duration_is_reversed(
            &ok("2025-01-05T10:00:00.50"),
            &ok("2025-01-05T10:00:00.5")
        ));
    }

    #[test]
    fn mixed_zone_is_rejected_only_when_certainly_reversed() {
        // Differ by far more than 14h: decidable.
        assert!(duration_is_reversed(
            &ok("2025-01-05T00:00:00"),
            &ok("2025-01-03T00:00:00Z")
        ));
        assert!(duration_is_reversed(
            &ok("2025-01-05T00:00:00Z"),
            &ok("2025-01-03T00:00:00")
        ));
        // Within the +-14h window: indeterminate, accepted.
        assert!(!duration_is_reversed(
            &ok("2025-01-05T10:00:00"),
            &ok("2025-01-05T09:00:00Z")
        ));
        assert!(!duration_is_reversed(
            &ok("2025-01-05T10:00:00Z"),
            &ok("2025-01-05T09:00:00")
        ));
    }
}
