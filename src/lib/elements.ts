// masses: IUPAC abridged standard atomic weights (conventional value where the standard one is an interval),
// rounded to at most 3 decimals. bracketed: no standard weight, so the mass number of the longest-lived isotope
export type ElementCategory =
  | 'alkali'
  | 'alkaline'
  | 'transition'
  | 'post'
  | 'metalloid'
  | 'nonmetal'
  | 'halogen'
  | 'noble'
  | 'lanthanide'
  | 'actinide'

export type Element = {
  n: number
  symbol: string
  name: string
  mass: string
  period: number
  // null for the f-block rows under the table
  group: number | null
  category: ElementCategory
}

type Row = [n: number, symbol: string, name: string, mass: string, period: number, group: number | null, category: ElementCategory]

const ROWS: Row[] = [
  [1, 'H', 'Hydrogen', '1.008', 1, 1, 'nonmetal'],
  [2, 'He', 'Helium', '4.003', 1, 18, 'noble'],

  [3, 'Li', 'Lithium', '6.94', 2, 1, 'alkali'],
  [4, 'Be', 'Beryllium', '9.012', 2, 2, 'alkaline'],
  [5, 'B', 'Boron', '10.81', 2, 13, 'metalloid'],
  [6, 'C', 'Carbon', '12.011', 2, 14, 'nonmetal'],
  [7, 'N', 'Nitrogen', '14.007', 2, 15, 'nonmetal'],
  [8, 'O', 'Oxygen', '15.999', 2, 16, 'nonmetal'],
  [9, 'F', 'Fluorine', '18.998', 2, 17, 'halogen'],
  [10, 'Ne', 'Neon', '20.180', 2, 18, 'noble'],

  [11, 'Na', 'Sodium', '22.990', 3, 1, 'alkali'],
  [12, 'Mg', 'Magnesium', '24.305', 3, 2, 'alkaline'],
  [13, 'Al', 'Aluminum', '26.982', 3, 13, 'post'],
  [14, 'Si', 'Silicon', '28.085', 3, 14, 'metalloid'],
  [15, 'P', 'Phosphorus', '30.974', 3, 15, 'nonmetal'],
  [16, 'S', 'Sulfur', '32.06', 3, 16, 'nonmetal'],
  [17, 'Cl', 'Chlorine', '35.45', 3, 17, 'halogen'],
  [18, 'Ar', 'Argon', '39.95', 3, 18, 'noble'],

  [19, 'K', 'Potassium', '39.098', 4, 1, 'alkali'],
  [20, 'Ca', 'Calcium', '40.078', 4, 2, 'alkaline'],
  [21, 'Sc', 'Scandium', '44.956', 4, 3, 'transition'],
  [22, 'Ti', 'Titanium', '47.867', 4, 4, 'transition'],
  [23, 'V', 'Vanadium', '50.942', 4, 5, 'transition'],
  [24, 'Cr', 'Chromium', '51.996', 4, 6, 'transition'],
  [25, 'Mn', 'Manganese', '54.938', 4, 7, 'transition'],
  [26, 'Fe', 'Iron', '55.845', 4, 8, 'transition'],
  [27, 'Co', 'Cobalt', '58.933', 4, 9, 'transition'],
  [28, 'Ni', 'Nickel', '58.693', 4, 10, 'transition'],
  [29, 'Cu', 'Copper', '63.546', 4, 11, 'transition'],
  [30, 'Zn', 'Zinc', '65.38', 4, 12, 'transition'],
  [31, 'Ga', 'Gallium', '69.723', 4, 13, 'post'],
  [32, 'Ge', 'Germanium', '72.630', 4, 14, 'metalloid'],
  [33, 'As', 'Arsenic', '74.922', 4, 15, 'metalloid'],
  [34, 'Se', 'Selenium', '78.971', 4, 16, 'nonmetal'],
  [35, 'Br', 'Bromine', '79.904', 4, 17, 'halogen'],
  [36, 'Kr', 'Krypton', '83.798', 4, 18, 'noble'],

  [37, 'Rb', 'Rubidium', '85.468', 5, 1, 'alkali'],
  [38, 'Sr', 'Strontium', '87.62', 5, 2, 'alkaline'],
  [39, 'Y', 'Yttrium', '88.906', 5, 3, 'transition'],
  [40, 'Zr', 'Zirconium', '91.222', 5, 4, 'transition'],
  [41, 'Nb', 'Niobium', '92.906', 5, 5, 'transition'],
  [42, 'Mo', 'Molybdenum', '95.95', 5, 6, 'transition'],
  [43, 'Tc', 'Technetium', '[98]', 5, 7, 'transition'],
  [44, 'Ru', 'Ruthenium', '101.07', 5, 8, 'transition'],
  [45, 'Rh', 'Rhodium', '102.91', 5, 9, 'transition'],
  [46, 'Pd', 'Palladium', '106.42', 5, 10, 'transition'],
  [47, 'Ag', 'Silver', '107.87', 5, 11, 'transition'],
  [48, 'Cd', 'Cadmium', '112.41', 5, 12, 'transition'],
  [49, 'In', 'Indium', '114.82', 5, 13, 'post'],
  [50, 'Sn', 'Tin', '118.71', 5, 14, 'post'],
  [51, 'Sb', 'Antimony', '121.76', 5, 15, 'metalloid'],
  [52, 'Te', 'Tellurium', '127.60', 5, 16, 'metalloid'],
  [53, 'I', 'Iodine', '126.90', 5, 17, 'halogen'],
  [54, 'Xe', 'Xenon', '131.29', 5, 18, 'noble'],

  [55, 'Cs', 'Cesium', '132.91', 6, 1, 'alkali'],
  [56, 'Ba', 'Barium', '137.33', 6, 2, 'alkaline'],
  [57, 'La', 'Lanthanum', '138.91', 6, null, 'lanthanide'],
  [58, 'Ce', 'Cerium', '140.12', 6, null, 'lanthanide'],
  [59, 'Pr', 'Praseodymium', '140.91', 6, null, 'lanthanide'],
  [60, 'Nd', 'Neodymium', '144.24', 6, null, 'lanthanide'],
  [61, 'Pm', 'Promethium', '[145]', 6, null, 'lanthanide'],
  [62, 'Sm', 'Samarium', '150.36', 6, null, 'lanthanide'],
  [63, 'Eu', 'Europium', '151.96', 6, null, 'lanthanide'],
  [64, 'Gd', 'Gadolinium', '157.25', 6, null, 'lanthanide'],
  [65, 'Tb', 'Terbium', '158.93', 6, null, 'lanthanide'],
  [66, 'Dy', 'Dysprosium', '162.50', 6, null, 'lanthanide'],
  [67, 'Ho', 'Holmium', '164.93', 6, null, 'lanthanide'],
  [68, 'Er', 'Erbium', '167.26', 6, null, 'lanthanide'],
  [69, 'Tm', 'Thulium', '168.93', 6, null, 'lanthanide'],
  [70, 'Yb', 'Ytterbium', '173.05', 6, null, 'lanthanide'],
  [71, 'Lu', 'Lutetium', '174.97', 6, null, 'lanthanide'],
  [72, 'Hf', 'Hafnium', '178.49', 6, 4, 'transition'],
  [73, 'Ta', 'Tantalum', '180.95', 6, 5, 'transition'],
  [74, 'W', 'Tungsten', '183.84', 6, 6, 'transition'],
  [75, 'Re', 'Rhenium', '186.21', 6, 7, 'transition'],
  [76, 'Os', 'Osmium', '190.23', 6, 8, 'transition'],
  [77, 'Ir', 'Iridium', '192.22', 6, 9, 'transition'],
  [78, 'Pt', 'Platinum', '195.08', 6, 10, 'transition'],
  [79, 'Au', 'Gold', '196.97', 6, 11, 'transition'],
  [80, 'Hg', 'Mercury', '200.59', 6, 12, 'transition'],
  [81, 'Tl', 'Thallium', '204.38', 6, 13, 'post'],
  [82, 'Pb', 'Lead', '207.2', 6, 14, 'post'],
  [83, 'Bi', 'Bismuth', '208.98', 6, 15, 'post'],
  [84, 'Po', 'Polonium', '[209]', 6, 16, 'post'],
  [85, 'At', 'Astatine', '[210]', 6, 17, 'halogen'],
  [86, 'Rn', 'Radon', '[222]', 6, 18, 'noble'],

  [87, 'Fr', 'Francium', '[223]', 7, 1, 'alkali'],
  [88, 'Ra', 'Radium', '[226]', 7, 2, 'alkaline'],
  [89, 'Ac', 'Actinium', '[227]', 7, null, 'actinide'],
  [90, 'Th', 'Thorium', '232.04', 7, null, 'actinide'],
  [91, 'Pa', 'Protactinium', '231.04', 7, null, 'actinide'],
  [92, 'U', 'Uranium', '238.03', 7, null, 'actinide'],
  [93, 'Np', 'Neptunium', '[237]', 7, null, 'actinide'],
  [94, 'Pu', 'Plutonium', '[244]', 7, null, 'actinide'],
  [95, 'Am', 'Americium', '[243]', 7, null, 'actinide'],
  [96, 'Cm', 'Curium', '[247]', 7, null, 'actinide'],
  [97, 'Bk', 'Berkelium', '[247]', 7, null, 'actinide'],
  [98, 'Cf', 'Californium', '[251]', 7, null, 'actinide'],
  [99, 'Es', 'Einsteinium', '[252]', 7, null, 'actinide'],
  [100, 'Fm', 'Fermium', '[257]', 7, null, 'actinide'],
  [101, 'Md', 'Mendelevium', '[258]', 7, null, 'actinide'],
  [102, 'No', 'Nobelium', '[259]', 7, null, 'actinide'],
  [103, 'Lr', 'Lawrencium', '[266]', 7, null, 'actinide'],
  [104, 'Rf', 'Rutherfordium', '[267]', 7, 4, 'transition'],
  [105, 'Db', 'Dubnium', '[268]', 7, 5, 'transition'],
  [106, 'Sg', 'Seaborgium', '[267]', 7, 6, 'transition'],
  [107, 'Bh', 'Bohrium', '[270]', 7, 7, 'transition'],
  [108, 'Hs', 'Hassium', '[271]', 7, 8, 'transition'],
  [109, 'Mt', 'Meitnerium', '[278]', 7, 9, 'transition'],
  [110, 'Ds', 'Darmstadtium', '[281]', 7, 10, 'transition'],
  [111, 'Rg', 'Roentgenium', '[282]', 7, 11, 'transition'],
  [112, 'Cn', 'Copernicium', '[285]', 7, 12, 'transition'],
  [113, 'Nh', 'Nihonium', '[286]', 7, 13, 'post'],
  [114, 'Fl', 'Flerovium', '[289]', 7, 14, 'post'],
  [115, 'Mc', 'Moscovium', '[290]', 7, 15, 'post'],
  [116, 'Lv', 'Livermorium', '[293]', 7, 16, 'post'],
  [117, 'Ts', 'Tennessine', '[294]', 7, 17, 'halogen'],
  [118, 'Og', 'Oganesson', '[294]', 7, 18, 'noble'],
]

export const ELEMENTS: readonly Element[] = ROWS.map(([n, symbol, name, mass, period, group, category]) => ({
  n,
  symbol,
  name,
  mass,
  period,
  group,
  category,
}))

export function hasStandardWeight(e: Element): boolean {
  return !e.mass.startsWith('[')
}

// what a click inserts: the weight, or a bracketed mass number without its brackets
export function insertableMass(e: Element): string {
  return e.mass.replace(/^\[(\d+)\]$/, '$1')
}

// 1-based grid cell: rows 1–7 are the table, row 8 is a gap, 9 and 10 hold the lanthanides and actinides from column 3
export function gridCell(e: Element): { row: number; col: number } {
  if (e.group != null) return { row: e.period, col: e.group }
  const first = e.period === 6 ? 57 : 89
  return { row: e.period + 3, col: 3 + e.n - first }
}
