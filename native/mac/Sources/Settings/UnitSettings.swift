import AppKit
import QCalcCore
import SwiftUI

// The Units tab: the same rows as macos/UnitSettings.swift, but the choices come from the engine
// (`unitSettingGroups()`, WP-UNITS) instead of a Swift copy of the table, and what's stored is cleaned by
// `sanitizeDefaultUnits`, so a unit the engine doesn't know for a dimension never sticks.
struct UnitSettingsView: View {
    @ObservedObject var settings: AppSettings
    private let groups = unitSettingGroups()

    var body: some View {
        Form {
            Section {
                LabeledContent {
                    Button("Reset") { settings.resetDefaultUnits(notifyWeb: true) }
                        .disabled(settings.defaultUnits.isEmpty)
                } label: {
                    Text("Default units")
                    Text("Quantities typed without “to …” convert into these. Automatic keeps the built-in SI ↔ US pair.")
                }
            }
            ForEach(groups, id: \.id) { group in
                Section(group.title) {
                    ForEach(group.items, id: \.dim) { item in
                        Picker(item.label, selection: binding(for: item.dim)) {
                            Text("Automatic").tag("")
                            Divider()
                            ForEach(item.units, id: \.id) { unit in
                                Text(unit.label).tag(unit.id)
                            }
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .onAppear {
            // settings written by an older build may name units the engine no longer offers there
            let clean = sanitizeDefaultUnits(units: settings.defaultUnits)
            if clean != settings.defaultUnits { settings.replaceDefaultUnits(clean, notifyWeb: true) }
        }
    }

    private func binding(for dim: String) -> Binding<String> {
        Binding(
            get: { settings.defaultUnits[dim] ?? "" },
            set: { id in
                var next = settings.defaultUnits
                next[dim] = id.isEmpty ? nil : id
                settings.replaceDefaultUnits(sanitizeDefaultUnits(units: next), notifyWeb: true)
            }
        )
    }
}
