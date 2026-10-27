//! Games (stub): pong, ember, connect 4, trails, ultimate, crack, equal, numbers. The Mac app now draws them
//! natively (native/mac/Sources/Games/*.swift over ffi/src/games*.rs); port those here, each in its own file in
//! this folder, in a game window sized by place.rs `game_window` / `game_window_min`.

/// Opens a game by its id (`pong`, `connect4`, `ultimate`, `ember`, `crack`, `equal`, `numbers`, `trails`), from
/// the `games` list or a game's name typed in the bar (commands.rs). The hook the games package fills in; until it
/// does, the bar says so.
pub fn open(app: &mut crate::app::App, kind: &str) {
    crate::qlog!("games: {kind} isn't in the native app yet");
    app.model.hint = Some(format!("{kind} isn't in the native app yet"));
    app.model.touch();
}
