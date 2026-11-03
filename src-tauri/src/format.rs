//! Human-friendly number formatting for the tray (tooltip, menu, macOS title)
//! and the "measurement stopped" notification.
//!
//! The panel uses the same rules, implemented in `src/format.ts`; keep both in
//! sync (they share the same test cases).
//!
//! Numbers stay ASCII (no grouping, plain space before the unit) except the
//! decimal separator, which follows the language: `23.4 W` / `23,4 W`.

use crate::i18n::{fill, tr, Key, Lang};

/// Placeholder shown for values that are not finite numbers.
const DASH: &str = "–";

/// Use the decimal separator of `lang` in a number formatted by `format!`.
fn localize(s: String, lang: Lang) -> String {
    match lang.decimal_separator() {
        '.' => s,
        sep => s.replace('.', &sep.to_string()),
    }
}

/// Format `x` with at most 3 significant digits, trailing zeros removed.
///
/// `0.8432 → "0.843"`, `12.345 → "12.3"`, `123.4 → "123"`, `0.84 → "0.84"`.
/// Values smaller than 1e-6 are shown as `"0"`.
pub fn sig3(x: f64, lang: Lang) -> String {
    localize(sig3_ascii(x), lang)
}

fn sig3_ascii(x: f64) -> String {
    if !x.is_finite() {
        return DASH.to_string();
    }
    if x.abs() < 1e-6 {
        return "0".to_string();
    }
    let magnitude = x.abs().log10().floor() as i32;
    let decimals = (2 - magnitude).clamp(0, 6) as usize;
    let s = format!("{x:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Power in watts, one decimal: `"23.4 W"`.
pub fn power(watts: f64, lang: Lang) -> String {
    if !watts.is_finite() {
        return DASH.to_string();
    }
    localize(format!("{watts:.1} W"), lang)
}

/// Energy (input in kWh): `"12.30 Wh"` below 1000 Wh, then `"1.20 kWh"`.
pub fn energy(kwh: f64, lang: Lang) -> String {
    if !kwh.is_finite() {
        return DASH.to_string();
    }
    let wh = kwh * 1000.0;
    let s = if wh.abs() < 999.995 {
        format!("{wh:.2} Wh")
    } else {
        format!("{kwh:.2} kWh")
    };
    localize(s, lang)
}

/// CO₂-equivalent (input in kg): `"0.84 g"` (3 significant digits) below
/// 1 kg, then `"1.2 kg"`.
pub fn co2(kg: f64, lang: Lang) -> String {
    if !kg.is_finite() {
        return DASH.to_string();
    }
    let g = kg * 1000.0;
    if g.abs() < 999.5 {
        format!("{} g", sig3(g, lang))
    } else {
        format!("{} kg", sig3(kg, lang))
    }
}

/// Duration as `hh:mm:ss` (hours may exceed 24).
pub fn duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "00:00:00".to_string();
    }
    let s = seconds.floor() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// A duration in words, like the panel's `durationShort`: `"45 s"`,
/// `"12 min"`, `"3 h 5 min"`, `"26 h"` (the same in English and French).
pub fn duration_short(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return DASH.to_string();
    }
    if seconds < 60.0 {
        return format!("{} s", seconds.floor());
    }
    let minutes = (seconds / 60.0).round() as u64;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Body of the "measurement stopped" notification:
/// `"Session: 12 min · 0.04 Wh · 0.003 g CO₂e"`.
pub fn session_notification(
    duration_s: f64,
    energy_kwh: f64,
    emissions_kg: f64,
    lang: Lang,
) -> String {
    fill(
        tr(lang, Key::NotifyStopped),
        &[
            &duration_short(duration_s),
            &energy(energy_kwh, lang),
            &co2(emissions_kg, lang),
        ],
    )
}

/// Grid carbon intensity as an integer: `"56 g/kWh"`.
pub fn intensity(g_per_kwh: f64) -> String {
    if !g_per_kwh.is_finite() {
        return DASH.to_string();
    }
    format!("{g_per_kwh:.0} g/kWh")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang::{En, Fr};

    #[test]
    fn significant_digits() {
        assert_eq!(sig3(0.84, En), "0.84");
        assert_eq!(sig3(0.8432, En), "0.843");
        assert_eq!(sig3(12.345, En), "12.3");
        assert_eq!(sig3(123.4, En), "123");
        assert_eq!(sig3(5.0, En), "5");
        assert_eq!(sig3(0.0, En), "0");
        assert_eq!(sig3(0.000123, En), "0.000123");
        assert_eq!(sig3(1e-9, En), "0");
        assert_eq!(sig3(f64::NAN, En), "–");
    }

    #[test]
    fn power_has_one_decimal() {
        assert_eq!(power(23.44, En), "23.4 W");
        assert_eq!(power(0.0, En), "0.0 W");
        assert_eq!(power(105.0, En), "105.0 W");
    }

    #[test]
    fn energy_switches_to_kwh() {
        assert_eq!(energy(0.0123, En), "12.30 Wh");
        assert_eq!(energy(0.0, En), "0.00 Wh");
        assert_eq!(energy(0.999, En), "999.00 Wh");
        assert_eq!(energy(1.2, En), "1.20 kWh");
        assert_eq!(energy(0.999999, En), "1.00 kWh");
    }

    #[test]
    fn co2_switches_to_kg() {
        assert_eq!(co2(0.00084, En), "0.84 g");
        assert_eq!(co2(0.0123456, En), "12.3 g");
        assert_eq!(co2(0.5, En), "500 g");
        assert_eq!(co2(0.9996, En), "1 kg");
        assert_eq!(co2(1.234, En), "1.23 kg");
        assert_eq!(co2(0.0, En), "0 g");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(0.0), "00:00:00");
        assert_eq!(duration(61.9), "00:01:01");
        assert_eq!(duration(3600.0 * 25.0 + 5.0), "25:00:05");
        assert_eq!(duration(-1.0), "00:00:00");
    }

    #[test]
    fn durations_in_words() {
        // Same cases as `durationShort` in src/format.test.ts.
        assert_eq!(duration_short(0.0), "0 s");
        assert_eq!(duration_short(45.9), "45 s");
        assert_eq!(duration_short(60.0), "1 min");
        assert_eq!(duration_short(12.0 * 60.0 + 10.0), "12 min");
        assert_eq!(duration_short(3.0 * 3600.0 + 5.0 * 60.0), "3 h 5 min");
        assert_eq!(duration_short(26.0 * 3600.0), "26 h");
        assert_eq!(duration_short(-1.0), "–");
        assert_eq!(duration_short(f64::NAN), "–");
    }

    #[test]
    fn notification_body() {
        assert_eq!(
            session_notification(725.0, 0.00004, 0.000003, En),
            "Session: 12 min · 0.04 Wh · 0.003 g CO₂e"
        );
        assert_eq!(
            session_notification(725.0, 0.00004, 0.000003, Fr),
            "Session\u{a0}: 12 min · 0,04 Wh · 0,003 g CO₂e"
        );
    }

    #[test]
    fn intensities() {
        assert_eq!(intensity(56.4), "56 g/kWh");
        assert_eq!(intensity(f64::INFINITY), "–");
    }

    #[test]
    fn french_uses_a_decimal_comma() {
        assert_eq!(sig3(0.8432, Fr), "0,843");
        assert_eq!(sig3(123.4, Fr), "123");
        assert_eq!(power(23.44, Fr), "23,4 W");
        assert_eq!(energy(0.0123, Fr), "12,30 Wh");
        assert_eq!(energy(1.2, Fr), "1,20 kWh");
        assert_eq!(co2(0.00084, Fr), "0,84 g");
        assert_eq!(co2(1.234, Fr), "1,23 kg");
        assert_eq!(power(f64::NAN, Fr), "–");
    }
}
