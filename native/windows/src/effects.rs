//! The easter-egg effects over the bar (stub): 420 smoke, 67, 69, barrel roll, black hole, π digits. Spec:
//! native/mac/Sources/Effects/EggOverlay.swift and ffi/src/games_ember.rs (the effects the core computes). An
//! effect may need room around the bar: place.rs `Room` and `resized` grow the window and give it back exactly.
//! Animations are the one place a timer may run while the bar is up; stop it when the effect ends.
