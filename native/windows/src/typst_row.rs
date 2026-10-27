//! The typeset maths row (stub) under the composer when `settings.typstPreview` is on and Typst would show
//! something the bar does not. Spec: native/mac/Sources/Bar/MathText.swift and BarModel.swift `refreshTypeset`.
//! The source comes from `Engine::typst_source` (Lane::Typeset); `EngineThread::typst` runs
//! `TypstRenderer::paths` on the Typst thread (already warmed at launch); draw the paths as a D2D path geometry
//! in the text colour (light/dark need no recompile). Mid-edit input Typst rejects keeps the last picture, dimmed.
