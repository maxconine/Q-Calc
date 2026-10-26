// engine/plainMath for the app-logic bundle: latexToAscii through the engine once it registers it; until then a
// LaTeX line is copied as Typst from its text unchanged (typstMath only calls this for LaTeX input)
import { engine } from './host'

export const latexToAscii = (latex: string): string => engine('engine/plainMath', 'latexToAscii', [latex], latex)
