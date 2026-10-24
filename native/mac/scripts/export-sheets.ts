// Writes the data the native app's reference sheets draw (the periodic table's tiles and the four identity
// sheets) in exactly the shapes the web page posts today (openNativePeriodicTable, openNativeIdentitySheet), so
// PeriodicWindow.swift and IdentityWindow.swift read it unchanged. Run by native/mac/scripts/export-sheets.sh.
//
// Stopgap until the core's elements() and identity_sheet() (WP-APP-LOGIC) are in QCalcCore; then the app asks the
// engine and this file and Resources/sheets.json go.
import { ELEMENTS, gridCell, insertableMass } from '../../../src/lib/elements'
import { CALCULUS_SHEET, CONSTANTS_SHEET, CONVERSION_SHEET, IDENTITY_SHEET, identityRuns, type IdentitySheet } from '../../../src/lib/identities'

const elements = ELEMENTS.map((e) => {
  const { row, col } = gridCell(e)
  return { n: e.n, symbol: e.symbol, name: e.name, mass: e.mass, insert: insertableMass(e), row, col, category: e.category }
})

const sheet = (s: IdentitySheet) => ({
  id: s.id,
  title: s.title,
  hint: s.hint,
  sections: s.sections.map((sec) => ({
    title: sec.title,
    wide: Boolean(sec.wide),
    groups: sec.groups.map((g) => ({
      title: g.title,
      items: g.items.map((item) => ({
        runs: identityRuns(item.show),
        copy: item.copy,
        note: item.note ?? '',
        lead: (item.lead ?? []).map(identityRuns),
      })),
    })),
  })),
})

const out = {
  version: 1,
  elements,
  sheets: [IDENTITY_SHEET, CALCULUS_SHEET, CONVERSION_SHEET, CONSTANTS_SHEET].map(sheet),
}
process.stdout.write(JSON.stringify(out))
