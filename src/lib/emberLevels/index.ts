import type { LevelDef } from '../emberTypes'
import { BASICS } from './basics'
import { DEPTHS } from './depths'
import { SUMMIT } from './summit'

// every shipped level, in the order the select screen shows them: easiest first. all of them can be picked
// from the start. ids are saved in progress: never change or reuse one.
// each pack has one owner (see EMBER2_PLAN.md) and its own test file next to it; this file is nobody's to edit
export const LEVELS: LevelDef[] = [...BASICS, ...DEPTHS, ...SUMMIT]
