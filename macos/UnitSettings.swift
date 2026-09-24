import AppKit
import SwiftUI

struct UnitChoice: Identifiable {
    let id: String
    let label: String
}

struct UnitSettingItem: Identifiable {
    var id: String { dim }
    let dim: String
    let label: String
    let units: [UnitChoice]
}

struct UnitSettingGroup: Identifiable {
    let id: String
    let title: String
    let items: [UnitSettingItem]
}

enum UnitCatalog {
    static let groups: [UnitSettingGroup] = [
        UnitSettingGroup(id: "length", title: "Length", items: [
            UnitSettingItem(dim: "length", label: "Length", units: [
                UnitChoice(id: "in", label: "Inch"),
                UnitChoice(id: "ft", label: "Foot"),
                UnitChoice(id: "yd", label: "Yard"),
                UnitChoice(id: "mi", label: "Mile"),
                UnitChoice(id: "mm", label: "Millimeter"),
                UnitChoice(id: "cm", label: "Centimeter"),
                UnitChoice(id: "m", label: "Meter"),
                UnitChoice(id: "km", label: "Kilometer"),
            ]),
            UnitSettingItem(dim: "area", label: "Area", units: [
                UnitChoice(id: "in2", label: "Square inch"),
                UnitChoice(id: "sqft", label: "Square foot"),
                UnitChoice(id: "yd2", label: "Square yard"),
                UnitChoice(id: "cm2", label: "Square centimeter"),
                UnitChoice(id: "m2", label: "Square meter"),
                UnitChoice(id: "km2", label: "Square kilometer"),
                UnitChoice(id: "acre", label: "Acre"),
                UnitChoice(id: "ha", label: "Hectare"),
            ]),
            UnitSettingItem(dim: "volume", label: "Volume", units: [
                UnitChoice(id: "in3", label: "Cubic inch"),
                UnitChoice(id: "ft3", label: "Cubic foot"),
                UnitChoice(id: "usgal", label: "US gallon"),
                UnitChoice(id: "floz", label: "US fluid ounce"),
                UnitChoice(id: "ml", label: "Milliliter"),
                UnitChoice(id: "l", label: "Liter"),
                UnitChoice(id: "m3", label: "Cubic meter"),
            ]),
            UnitSettingItem(dim: "speed", label: "Speed", units: [
                UnitChoice(id: "mph", label: "Mile per hour"),
                UnitChoice(id: "fps", label: "Foot per second"),
                UnitChoice(id: "kmh", label: "Kilometer per hour"),
                UnitChoice(id: "mps", label: "Meter per second"),
                UnitChoice(id: "knot", label: "Knot"),
            ]),
            UnitSettingItem(dim: "acceleration", label: "Linear acceleration", units: [
                UnitChoice(id: "mps2", label: "Meter per second squared"),
                UnitChoice(id: "gee", label: "g (standard gravity)"),
            ]),
        ]),
        UnitSettingGroup(id: "angle", title: "Angle", items: [
            UnitSettingItem(dim: "angle", label: "Angle", units: [
                UnitChoice(id: "deg", label: "Degree"),
                UnitChoice(id: "rad", label: "Radian"),
                UnitChoice(id: "rev", label: "Revolution"),
            ]),
        ]),
        UnitSettingGroup(id: "mechanical", title: "Mechanical", items: [
            UnitSettingItem(dim: "mass", label: "Mass", units: [
                UnitChoice(id: "oz", label: "Ounce"),
                UnitChoice(id: "lb", label: "Pound"),
                UnitChoice(id: "g", label: "Gram"),
                UnitChoice(id: "kg", label: "Kilogram"),
                UnitChoice(id: "ton", label: "US ton"),
                UnitChoice(id: "tonne", label: "Metric ton"),
            ]),
            UnitSettingItem(dim: "force", label: "Force", units: [
                UnitChoice(id: "ozf", label: "Ounce-force"),
                UnitChoice(id: "lbf", label: "Pound-force"),
                UnitChoice(id: "n", label: "Newton"),
                UnitChoice(id: "kilonewton", label: "Kilonewton"),
                UnitChoice(id: "kgf", label: "Kilogram-force"),
            ]),
            UnitSettingItem(dim: "pressure", label: "Pressure", units: [
                UnitChoice(id: "psi", label: "Pound per square inch"),
                UnitChoice(id: "ksi", label: "Kilopound per square inch"),
                UnitChoice(id: "kpa", label: "Kilopascal"),
                UnitChoice(id: "mpa", label: "Megapascal"),
                UnitChoice(id: "bar", label: "Bar"),
                UnitChoice(id: "atm", label: "Atmosphere"),
            ]),
            UnitSettingItem(dim: "energy", label: "Energy", units: [
                UnitChoice(id: "j", label: "Joule"),
                UnitChoice(id: "kj", label: "Kilojoule"),
                UnitChoice(id: "cal", label: "Calorie"),
                UnitChoice(id: "kcal", label: "Kilocalorie"),
                UnitChoice(id: "btu", label: "British thermal unit"),
                UnitChoice(id: "kwh", label: "Kilowatt-hour"),
            ]),
            UnitSettingItem(dim: "power", label: "Power", units: [
                UnitChoice(id: "w", label: "Watt"),
                UnitChoice(id: "kw", label: "Kilowatt"),
                UnitChoice(id: "hp", label: "Horsepower"),
            ]),
        ]),
        UnitSettingGroup(id: "temperature", title: "Temperature", items: [
            UnitSettingItem(dim: "temperature", label: "Temperature", units: [
                UnitChoice(id: "c", label: "Celsius"),
                UnitChoice(id: "f", label: "Fahrenheit"),
                UnitChoice(id: "k", label: "Kelvin"),
                UnitChoice(id: "r", label: "Rankine"),
            ]),
        ]),
        UnitSettingGroup(id: "time", title: "Time", items: [
            UnitSettingItem(dim: "time", label: "Time", units: [
                UnitChoice(id: "s", label: "Second"),
                UnitChoice(id: "min", label: "Minute"),
                UnitChoice(id: "hr", label: "Hour"),
                UnitChoice(id: "day", label: "Day"),
            ]),
            UnitSettingItem(dim: "frequency", label: "Frequency", units: [
                UnitChoice(id: "hz", label: "Hertz"),
                UnitChoice(id: "rpm", label: "Revolution per minute"),
            ]),
        ]),
    ]
}

struct UnitSettingsView: View {
    @ObservedObject var settings: AppSettings

    var body: some View {
        Form {
            Section {
                LabeledContent {
                    Button("Reset") {
                        settings.resetDefaultUnits(notifyWeb: true)
                    }
                    .disabled(settings.defaultUnits.isEmpty)
                } label: {
                    Text("Default units")
                    Text("Quantities typed without “to …” convert into these. Automatic keeps the built-in SI ↔ US pair.")
                }
            }
            ForEach(UnitCatalog.groups) { group in
                Section(group.title) {
                    ForEach(group.items) { item in
                        Picker(item.label, selection: binding(for: item.dim)) {
                            Text("Automatic").tag("")
                            Divider()
                            ForEach(item.units) { unit in
                                Text(unit.label).tag(unit.id)
                            }
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
    }

    private func binding(for dim: String) -> Binding<String> {
        Binding(
            get: { settings.defaultUnits[dim] ?? "" },
            set: { settings.setDefaultUnit(dim: dim, unitId: $0, notifyWeb: true) }
        )
    }
}
