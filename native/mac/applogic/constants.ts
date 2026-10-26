// Run at bundle time (node): the constants the shims copy from the real engine modules.
import { GAS_CONSTANT_UNITS } from '../../../src/engine/chem'
import { SCIENTIFIC_NAMES } from '../../../src/engine/scientific'

process.stdout.write(JSON.stringify({ SCIENTIFIC_NAMES, GAS_CONSTANT_UNITS }))
