//! The games and the bar's easter eggs, ported one-to-one from the TS (`src/lib/ember*.ts`, `src/lib/trails*.ts`,
//! `src/lib/{fourTwenty,sixtyNine,sixtySeven,smokeRoom}.ts`). Pure and deterministic: the Mac app's Swift owns
//! the clock, the keys, the drawing and the sound, and calls in here for everything the game decides.

pub mod eggs;
pub mod ember;
pub mod hello;
pub mod trails;
