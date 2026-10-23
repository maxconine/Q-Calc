//! PORT(WP-UNITS): `src/engine/units.ts` — the unit table, dimensions, prefixes, the unit reader `tryConvert`,
//! default display units, `quantityText`, `unitAlternatives`, `stepPrefix`, SI helpers, and the unit settings
//! groups. Until then every function here answers "not a unit", so lines with units stay blank.

use std::collections::BTreeMap;

use crate::types::{AngleMode, DefaultUnits, Value};

pub const IMPROPER_UNIT_CONVERSION: &str = "improper unit conversion";

pub fn is_improper_unit_conversion(text: &str) -> bool {
    text.trim().to_lowercase() == IMPROPER_UNIT_CONVERSION
}

/// The unit reader: a quantity, a conversion or unit arithmetic, or None when the text has no units.
pub fn try_convert(
    _text: &str,
    _defaults: &DefaultUnits,
    _variables: &BTreeMap<String, f64>,
    _keep_units: bool,
    _angle_mode: AngleMode,
) -> Option<Value> {
    None
}

/// Whether a word reads as a unit symbol or name (`V`, `kg`, `meters`).
pub fn reads_as_unit(_word: &str) -> bool {
    false
}

/// A unit answer as text the unit reader reads back (`5 cm`).
pub fn quantity_text(_value: &Value, _angle_mode: AngleMode) -> Option<String> {
    None
}

/// The same quantity in other units, for Tab.
pub fn unit_alternatives(_value: &Value, _limit: usize) -> Vec<Value> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    #[test]
    fn improper_conversions_are_recognised() {
        assert!(super::is_improper_unit_conversion(
            " Improper unit conversion "
        ));
    }
}
