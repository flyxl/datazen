//! PostgreSQL base-10000 NUMERIC decoding without a floating-point/96-bit ceiling.
use datazen_driver_api::DriverError;

pub(crate) fn binary_numeric_text(bytes: &[u8]) -> Result<String, DriverError> {
    let invalid = || DriverError::QueryFailed("invalid PostgreSQL numeric encoding".into());
    if bytes.len() < 8 {
        return Err(invalid());
    }
    let word = |offset| u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
    let count = word(0) as usize;
    let weight = word(2) as i16 as i32;
    let sign = word(4);
    let scale = word(6) as usize;
    if bytes.len() != 8 + count * 2 || scale > 16383 {
        return Err(invalid());
    }
    match sign {
        0xc000 => return Ok("NaN".into()),
        0xd000 => return Ok("Infinity".into()),
        0xf000 => return Ok("-Infinity".into()),
        0 | 0x4000 => {}
        _ => return Err(invalid()),
    }
    let digits: Vec<u16> = (0..count).map(|index| word(8 + index * 2)).collect();
    if digits.iter().any(|digit| *digit >= 10000) {
        return Err(invalid());
    }
    let group = |exponent: i32| -> u16 {
        let index = weight - exponent;
        if index < 0 {
            0
        } else {
            digits.get(index as usize).copied().unwrap_or(0)
        }
    };
    let mut text = String::new();
    if sign == 0x4000 && digits.iter().any(|digit| *digit != 0) {
        text.push('-');
    }
    if weight < 0 {
        text.push('0');
    } else {
        text.push_str(&group(weight).to_string());
        for exponent in (0..weight).rev() {
            text.push_str(&format!("{:04}", group(exponent)));
        }
    }
    if scale > 0 {
        text.push('.');
        let start = text.len();
        for index in 1..=scale.div_ceil(4) {
            text.push_str(&format!("{:04}", group(-(index as i32))));
        }
        text.truncate(start + scale);
    }
    Ok(text)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn encode(weight: i16, sign: u16, scale: u16, digits: &[u16]) -> Vec<u8> {
        [(digits.len() as u16), weight as u16, sign, scale]
            .into_iter()
            .chain(digits.iter().copied())
            .flat_map(u16::to_be_bytes)
            .collect()
    }
    #[test]
    fn exact_fraction_large_precision_scale_and_special_values() {
        assert_eq!(
            binary_numeric_text(&encode(0, 0, 8, &[1, 2345, 6789])).unwrap(),
            "1.23456789"
        );
        assert_eq!(
            binary_numeric_text(&encode(-2, 0x4000, 8, &[1234])).unwrap(),
            "-0.00001234"
        );
        assert_eq!(
            binary_numeric_text(&encode(8, 0, 4, &[9999; 10])).unwrap(),
            "999999999999999999999999999999999999.9999"
        );
        assert_eq!(binary_numeric_text(&encode(0, 0, 3, &[])).unwrap(), "0.000");
        assert_eq!(
            binary_numeric_text(&encode(0, 0xc000, 0, &[])).unwrap(),
            "NaN"
        );
        assert!(binary_numeric_text(&encode(0, 0, 0, &[10000])).is_err());
        assert!(binary_numeric_text(&[0; 3]).is_err());
    }
}
