import { isCalculusInput } from '../engine/calculus'
import { type CurrencyRates, isRatesTable } from '../engine/currency'
import { answerPhrase } from '../engine/phrases'
import { nativeWindow } from './bridge'
import { looksLikeNaturalLanguage } from './nativeEval'

type RatesWindow = { __QCALC_RATES?: unknown }

// the mac always has soulvercore; the phrase layer is only for hosts without it
export function hasSoulver(): boolean {
  return Boolean(nativeWindow()?.webkit?.messageHandlers?.soulver)
}

// a host may hand in live rates as { base, rates }; without them currency conversions stay blank
export function hostRates(): CurrencyRates | null {
  const v = (nativeWindow() as RatesWindow | undefined)?.__QCALC_RATES
  return isRatesTable(v) ? v : null
}

type Live = { display: string; n?: number }

// same priority as a soulvercore reply: it beats js on natural language, and otherwise only fills a blank
export function withPhraseAnswer(
  expr: string,
  jsDisplay: string,
  merged: Live,
  options: { enabled: boolean; sigFigs?: number; now?: Date; rates?: CurrencyRates | null },
): Live {
  if (!options.enabled || !expr.trim() || isCalculusInput(expr)) return merged
  // a real native reply already won
  if (merged.display && merged.display !== jsDisplay) return merged
  if (jsDisplay && !looksLikeNaturalLanguage(expr)) return merged
  const phrase = answerPhrase(expr, {
    now: options.now,
    rates: options.rates === undefined ? hostRates() : options.rates,
    sigFigs: options.sigFigs,
  })
  return phrase ? { display: phrase.display, n: phrase.n } : merged
}
