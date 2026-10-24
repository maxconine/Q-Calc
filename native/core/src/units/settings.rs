//! The unit choices Settings offers (`UNIT_SETTING_GROUPS` in `src/engine/units.ts`) and the sanitising of a
//! stored default-units map. The Mac UI reads the groups from here (through the FFI) instead of keeping its own
//! copy (`UnitCatalog` in `macos/UnitSettings.swift`).

use serde::Serialize;
use serde_json::Value as Json;

use super::table::{self, Dim};
use crate::types::DefaultUnits;

/// One unit a setting can pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct UnitChoice {
    pub id: &'static str,
    pub label: &'static str,
}

/// One setting: the default unit for one dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct UnitSettingItem {
    /// The dimension key (`length`), as in the stored map.
    pub dim: &'static str,
    pub label: &'static str,
    pub units: &'static [UnitChoice],
}

/// A Settings section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct UnitSettingGroup {
    pub id: &'static str,
    pub title: &'static str,
    pub items: &'static [UnitSettingItem],
}

const fn c(id: &'static str, label: &'static str) -> UnitChoice {
    UnitChoice { id, label }
}

/// Curated defaults shown in Settings. Automatic (empty) keeps SI ↔ US counterparts.
pub const UNIT_SETTING_GROUPS: &[UnitSettingGroup] = &[
    UnitSettingGroup {
        id: "length",
        title: "Length",
        items: &[
            UnitSettingItem {
                dim: "length",
                label: "Length",
                units: &[
                    c("in", "Inch"),
                    c("ft", "Foot"),
                    c("yd", "Yard"),
                    c("mi", "Mile"),
                    c("mm", "Millimeter"),
                    c("cm", "Centimeter"),
                    c("m", "Meter"),
                    c("km", "Kilometer"),
                ],
            },
            UnitSettingItem {
                dim: "area",
                label: "Area",
                units: &[
                    c("in2", "Square inch"),
                    c("sqft", "Square foot"),
                    c("yd2", "Square yard"),
                    c("cm2", "Square centimeter"),
                    c("m2", "Square meter"),
                    c("km2", "Square kilometer"),
                    c("acre", "Acre"),
                    c("ha", "Hectare"),
                ],
            },
            UnitSettingItem {
                dim: "volume",
                label: "Volume",
                units: &[
                    c("in3", "Cubic inch"),
                    c("ft3", "Cubic foot"),
                    c("usgal", "US gallon"),
                    c("floz", "US fluid ounce"),
                    c("ml", "Milliliter"),
                    c("l", "Liter"),
                    c("m3", "Cubic meter"),
                ],
            },
            UnitSettingItem {
                dim: "speed",
                label: "Speed",
                units: &[
                    c("mph", "Mile per hour"),
                    c("fps", "Foot per second"),
                    c("kmh", "Kilometer per hour"),
                    c("mps", "Meter per second"),
                    c("knot", "Knot"),
                ],
            },
            UnitSettingItem {
                dim: "acceleration",
                label: "Linear acceleration",
                units: &[
                    c("mps2", "Meter per second squared"),
                    c("gee", "g (standard gravity)"),
                ],
            },
        ],
    },
    UnitSettingGroup {
        id: "angle",
        title: "Angle",
        items: &[UnitSettingItem {
            dim: "angle",
            label: "Angle",
            units: &[c("deg", "Degree"), c("rad", "Radian"), c("rev", "Revolution")],
        }],
    },
    UnitSettingGroup {
        id: "mechanical",
        title: "Mechanical",
        items: &[
            UnitSettingItem {
                dim: "mass",
                label: "Mass",
                units: &[
                    c("oz", "Ounce"),
                    c("lb", "Pound"),
                    c("g", "Gram"),
                    c("kg", "Kilogram"),
                    c("ton", "US ton"),
                    c("tonne", "Metric ton"),
                ],
            },
            UnitSettingItem {
                dim: "force",
                label: "Force",
                units: &[
                    c("ozf", "Ounce-force"),
                    c("lbf", "Pound-force"),
                    c("n", "Newton"),
                    c("kilonewton", "Kilonewton"),
                    c("kgf", "Kilogram-force"),
                ],
            },
            UnitSettingItem {
                dim: "pressure",
                label: "Pressure",
                units: &[
                    c("psi", "Pound per square inch"),
                    c("ksi", "Kilopound per square inch"),
                    c("kpa", "Kilopascal"),
                    c("mpa", "Megapascal"),
                    c("bar", "Bar"),
                    c("atm", "Atmosphere"),
                ],
            },
            UnitSettingItem {
                dim: "energy",
                label: "Energy",
                units: &[
                    c("j", "Joule"),
                    c("kj", "Kilojoule"),
                    c("cal", "Calorie"),
                    c("kcal", "Kilocalorie"),
                    c("btu", "British thermal unit"),
                    c("kwh", "Kilowatt-hour"),
                ],
            },
            UnitSettingItem {
                dim: "power",
                label: "Power",
                units: &[c("w", "Watt"), c("kw", "Kilowatt"), c("hp", "Horsepower")],
            },
        ],
    },
    UnitSettingGroup {
        id: "temperature",
        title: "Temperature",
        items: &[UnitSettingItem {
            dim: "temperature",
            label: "Temperature",
            units: &[
                c("c", "Celsius"),
                c("f", "Fahrenheit"),
                c("k", "Kelvin"),
                c("r", "Rankine"),
            ],
        }],
    },
    UnitSettingGroup {
        id: "time",
        title: "Time",
        items: &[
            UnitSettingItem {
                dim: "time",
                label: "Time",
                units: &[
                    c("s", "Second"),
                    c("min", "Minute"),
                    c("hr", "Hour"),
                    c("day", "Day"),
                ],
            },
            UnitSettingItem {
                dim: "frequency",
                label: "Frequency",
                units: &[c("hz", "Hertz"), c("rpm", "Revolution per minute")],
            },
        ],
    },
];

/// Every setting, in Settings order.
pub fn setting_items() -> impl Iterator<Item = &'static UnitSettingItem> {
    UNIT_SETTING_GROUPS.iter().flat_map(|g| g.items.iter())
}

/// `SETTING_DIMS`: the dimensions Settings covers.
pub fn is_setting_dim(dim: &str) -> bool {
    setting_items().any(|i| i.dim == dim)
}

fn keep(dim: &str, id: &str) -> bool {
    if !is_setting_dim(dim) || id.is_empty() {
        return false;
    }
    table::by_id(id).is_some_and(|u| Dim::from_name(dim) == Some(u.dim))
}

/// `sanitizeDefaultUnits` on a typed map: only settings dimensions, each with a table unit of that dimension.
pub fn sanitize_default_units(raw: &DefaultUnits) -> DefaultUnits {
    raw.iter()
        .filter(|(dim, id)| keep(dim, id))
        .map(|(d, i)| (d.clone(), i.clone()))
        .collect()
}

/// `sanitizeDefaultUnits` on stored JSON of any shape (a string, an array, junk ids): what is left is valid.
pub fn sanitize_default_units_json(raw: &Json) -> DefaultUnits {
    let Json::Object(o) = raw else {
        return DefaultUnits::new();
    };
    o.iter()
        .filter_map(|(dim, id)| {
            let id = id.as_str()?;
            keep(dim, id).then(|| (dim.clone(), id.to_string()))
        })
        .collect()
}

/// `defaultUnitsEqual`.
pub fn default_units_equal(a: &DefaultUnits, b: &DefaultUnits) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitising_keeps_only_valid_choices() {
        let out = sanitize_default_units_json(&json!({"length": "ft", "mass": "ft", "speed": 3, "digital": "kb"}));
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("length").map(String::as_str), Some("ft"));
        assert!(sanitize_default_units_json(&json!("a string")).is_empty());
        // every listed choice is a table unit of its own dimension
        for item in setting_items() {
            for u in item.units {
                assert!(keep(item.dim, u.id), "{} {}", item.dim, u.id);
            }
        }
    }
}
