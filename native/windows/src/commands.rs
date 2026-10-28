//! The bar's word commands that aren't maths (`BarCommand` in native/mac/Sources/Bar/TextRules.swift), the ones
//! this package owns: `periodic`, the identity sheets (`identities`, `idt <part>`, `trig`, `constants`,
//! `conversions`, `metric prefixes`...), `games` and a game's name, `message <name> <text>`, `tutorial`, `skip`
//! and the `hello` greeting. The rules themselves are the core's (`qcalc_core::app::{games, identities,
//! messaging, tour, greeting}`), so the web, Mac and Windows read a line the same way.
//!
//! While a command is typed the live answer isn't asked for: what the command does shows in the answer slot,
//! 13 pt muted (`.live.message`), and ↵ runs it instead of saving a row (`live`, `enter`, called from model.rs).
//! `help`, `settings` and `graph` belong to other packages and are left to them (`parse` returns None).

use qcalc_core::app::games::{self as core_games, GAMES, GameKind};
use qcalc_core::app::{greeting, identities, messaging as core_msg, tour as core_tour};

/// A command line, as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Periodic,
    /// an identity sheet (`identities`, `calculus`, `conversions`, `constants`) opened at a section
    Identities {
        id: String,
        section: usize,
        hint: String,
    },
    /// the games list above the bar
    Games,
    Game(GameKind),
    /// `to` as typed, `name` cleaned ("" when it can't be one), the message
    Message {
        to: String,
        name: String,
        text: String,
    },
    /// `paste to <name>`, `send clipboard to <name>`, `send file to <name>` (share.rs): `what` is `clip` or `file`
    Share {
        what: String,
        to: String,
        name: String,
    },
    Tutorial,
    Skip,
    Greeting,
}

/// What the bar shows for a command line: the command and the words in the answer slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub command: Command,
    pub message: String,
}

/// The id `games::open` takes for a game (`pong`, `connect4`...), as the Mac's `onGame` and games.ts name them.
pub fn game_id(kind: GameKind) -> &'static str {
    match kind {
        GameKind::Pong => "pong",
        GameKind::Connect4 => "connect4",
        GameKind::Ember => "ember",
        GameKind::Crack => "crack",
        GameKind::Equal => "equal",
        GameKind::Numbers => "numbers",
        GameKind::Trails => "trails",
        GameKind::Ultimate => "ultimate",
    }
}

/// The command a line is, in the Mac's order (`BarCommand.of`), or None for maths (and for the other packages'
/// commands). Cheap enough for every keystroke: a line with no letter is never a command, and the rules are the
/// core's precompiled ones.
pub fn parse(raw: &str) -> Option<Command> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.bytes().any(|b| b.is_ascii_alphabetic()) || !could_be(trimmed)
    {
        return None;
    }
    // a command is a few words; a long line is maths (or a message, checked on the raw text below)
    let short = trimmed.len() <= 64;
    if short {
        let t = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        if core_games::is_periodic_command(&t) {
            return Some(Command::Periodic);
        }
        if let Some(target) = identities::identity_target_for(&t) {
            return Some(Command::Identities {
                id: target.sheet.id,
                section: target.section,
                hint: target.hint,
            });
        }
        if core_games::is_games_command(&t) {
            return Some(Command::Games);
        }
        if let Some(kind) = core_games::game_command(&t) {
            return Some(Command::Game(kind));
        }
    }
    if core_msg::is_message_command(raw)
        && let Some(line) = core_msg::parse_message(raw)
    {
        return Some(Command::Message {
            to: line.to,
            name: line.name,
            text: line.text,
        });
    }
    if let Some(share) = core_msg::parse_share_command(raw) {
        return Some(Command::Share {
            what: share.what,
            to: share.to,
            name: share.name,
        });
    }
    if !short {
        return None;
    }
    if core_tour::is_tutorial_command(trimmed) {
        return Some(Command::Tutorial);
    }
    if core_tour::is_skip_command(trimmed) {
        return Some(Command::Skip);
    }
    if greeting::is_greeting(trimmed) {
        return Some(Command::Greeting);
    }
    None
}

/// The first words a command of ours can start with: a line whose first word starts none of them is maths, and the
/// rules' regexes (compiled at their first use, a few hundred KB) are never asked. Keep in step with the core's
/// `games`, `identities`, `messaging`, `tour` and `greeting` rules.
const STEMS: [&str; 41] = [
    "periodic",
    "identit",
    "idt",
    "trig",
    "log",
    "exponent",
    "algebra",
    "math",
    "conversion",
    "unit",
    "metric",
    "si",
    "prefix",
    "common",
    "derivative",
    "differentiation",
    "integra",
    "antiderivative",
    "calculus",
    "physic",
    "fundamental",
    "constant",
    "codata",
    "partial",
    "game",
    "pong",
    "connect",
    "ultimate",
    "uttt",
    "ember",
    "frost",
    "fire",
    "crack",
    "safe",
    "equal",
    "nerdle",
    "24",
    "count",
    "numbers",
    "trail",
    "cycles",
];

const MORE_STEMS: [&str; 9] = [
    "message", "tutorial", "tour", "skip", "hello", "hiya", "send", "paste", "share",
];

fn could_be(trimmed: &str) -> bool {
    let first: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    !first.is_empty()
        && (STEMS
            .iter()
            .chain(&MORE_STEMS)
            .any(|s| first.starts_with(s))
            || first == "howdy")
}

/// The words in the answer slot while `command` is typed (`BarCommand.hint`). `message` is the message line's
/// hint, from messaging.rs (it depends on your name and the relay's word on theirs).
pub fn hint(command: &Command, message: impl FnOnce(&str, &str, &str) -> String) -> String {
    match command {
        Command::Periodic => core_games::PERIODIC_HINT.into(),
        Command::Identities { hint, .. } => hint.clone(),
        Command::Games => core_games::GAMES_HINT.into(),
        Command::Game(kind) => core_games::game_hint(*kind).into(),
        Command::Message { to, name, text } => message(to, name, text),
        // share.rs's words, worked out in `live`, where the app is
        Command::Share { .. } => String::new(),
        Command::Tutorial => core_tour::TUTORIAL_HINT.into(),
        Command::Skip => String::new(),
        Command::Greeting => greeting::GREETING_REPLY.into(),
    }
}

/// The games list's rows: (id, name, blurb).
pub fn game_rows() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    GAMES.iter().map(|g| (game_id(g.kind), g.name, g.blurb))
}

pub const GAME_COUNT: usize = GAMES.len();

/// ↑ ↓ in the games list: ↑ starts at the game nearest the bar (the last) and climbs; ↓ past the last leaves it.
pub fn games_step(pick: Option<usize>, up: bool) -> Option<usize> {
    let last = GAME_COUNT - 1;
    match (pick, up) {
        (None, true) => Some(last),
        (Some(i), true) => Some(i.saturating_sub(1)),
        (None, false) => None,
        (Some(i), false) if i < last => Some(i + 1),
        (Some(_), false) => None,
    }
}

#[cfg(windows)]
pub use ui::*;

#[cfg(windows)]
mod ui {
    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    };

    use super::{Command, Shown, game_id, game_rows, hint, parse};
    use crate::app::App;
    use crate::engine::{EngineThread, Lane};
    use crate::model::BarModel;
    use crate::render::{Canvas, Gfx, Rect, measure, size, with_alpha};
    use crate::{games, messaging, sheets, tour};

    /// The games list's row height and its name column (`.cheat-row.game-row`).
    pub const GAME_ROW: f32 = 28.0;
    const GAME_NAME_W: f32 = 126.0;

    /// From `BarModel::evaluate_live`: a command line shows its words in the answer slot and asks the engine for
    /// nothing. True when the line was a command (the live answer is then blank).
    pub fn live(m: &mut BarModel, engine: &EngineThread) -> bool {
        let typed = m.field.text().to_string();
        let Some(command) = parse(&typed) else {
            if m.command.take().is_some() {
                m.games_pick = None;
                m.touch();
            }
            return false;
        };
        let message = match &command {
            Command::Share { what, to, name } => crate::share::hint(what, to, name),
            _ => hint(&command, |to, name, text| {
                messaging::line_hint(to, name, text)
            }),
        };
        if !matches!(command, Command::Games) {
            m.games_pick = None;
        }
        if let Command::Message { name, .. } = &command {
            messaging::check_name(name);
        }
        // a game's `↵ play pong` shows here in the slot, so not again in the hint (games::hint, model.rs)
        if matches!(command, Command::Game(_)) && m.hint.is_some() && m.hint == games::hint(&typed)
        {
            m.hint = None;
        }
        m.command = Some(Shown { command, message });
        engine.cancel(Lane::Live);
        m.chained = false;
        m.live = None;
        m.live_for = typed;
        m.touch();
        true
    }

    /// The games list is up (`games` typed).
    pub fn games_shown(m: &BarModel) -> bool {
        matches!(
            m.command,
            Some(Shown {
                command: Command::Games,
                ..
            })
        )
    }

    /// From `BarModel::enter`: ↵ on a command line runs it. True when it was one (nothing is saved).
    pub fn enter(app: &mut App) -> bool {
        if games_shown(&app.model) {
            if let Some(i) = app.model.games_pick
                && let Some((id, ..)) = game_rows().nth(i)
            {
                play(app, id);
            }
            // with nothing picked, ↵ on `games` does nothing (the Mac commits the word, which saves nothing)
            return true;
        }
        let Some(command) = parse(app.model.field.text()) else {
            return false;
        };
        match command {
            Command::Periodic => {
                clear_line(app, false);
                sheets::open_periodic(app);
            }
            Command::Identities { id, section, .. } => {
                clear_line(app, false);
                sheets::open_identities(app, &id, section);
            }
            Command::Games => {}
            Command::Game(kind) => play(app, game_id(kind)),
            Command::Message { name, text, .. } => {
                if !messaging::has_name() {
                    clear_line(app, false);
                    crate::settings_window::open(app);
                    return true;
                }
                if name.is_empty() || text.is_empty() {
                    return true;
                }
                messaging::send_line(app, name, text);
            }
            Command::Share { what, name, .. } => crate::share::enter(app, &what, &name),
            Command::Tutorial => {
                clear_line(app, false);
                tour::start(app);
            }
            Command::Skip => {
                clear_line(app, false);
                tour::skip(app);
            }
            Command::Greeting => clear_line(app, false),
        }
        true
    }

    /// Opens a game in the games window (games/mod.rs) and puts the bar away, as a game's name and ↵ does there;
    /// one the native app doesn't have yet says so.
    fn play(app: &mut App, id: &'static str) {
        clear_line(app, false);
        if games::list().iter().any(|(kind, _)| *kind == id) {
            // the game takes the bar's place: its width, hanging from the composer's top (the Mac's GameBarSpot)
            let spot = app.bar.visible.then(|| {
                let f = app.bar.frame();
                games::BarSpot {
                    x: f.left,
                    width: f.right - f.left,
                    top: f.top + (app.bar.layout.composer.y * app.bar.scale()).round() as i32,
                }
            });
            games::open_at(id, spot);
            app.ops.push(crate::app::Op::Hide);
        } else {
            crate::qlog!("games: {id} isn't in the native app yet");
            app.model.hint = Some(format!("{id} isn't in the native app yet"));
            app.model.touch();
        }
    }

    /// `clearLine`: the line goes (and the tape's selection), keeping the hint if asked.
    pub fn clear_line(app: &mut App, keep_hint: bool) {
        let hint = app.model.hint.clone();
        app.model.field.reset("");
        app.model.selected = None;
        app.model.tape_open = false;
        app.text_changed();
        if keep_hint {
            app.model.hint = hint;
        }
        app.model.touch();
    }

    /// ↑ ↓ while the games list is up; true when it took the key.
    pub fn games_key(m: &mut BarModel, up: bool) -> bool {
        if !games_shown(m) {
            return false;
        }
        m.games_pick = super::games_step(m.games_pick, up);
        m.touch();
        true
    }

    // ---- the bar's input, for this package's parts (one call each from app.rs) ----

    /// A key-down in the bar, before the model's own keys: a reply line's keys, Esc for the cards and the games
    /// list, ↵ for the periodic table's formula and the commands, ↑ ↓ in the games list. True when it was taken.
    pub fn key_down(app: &mut App, vk: u32, mods: crate::keys::Mods) -> bool {
        const ENTER: u32 = 0x0D;
        const ESC: u32 = 0x1B;
        const UP: u32 = 0x26;
        const DOWN: u32 = 0x28;
        crate::hover::key_down();
        if messaging::key(app, vk, mods) {
            return true;
        }
        // Ctrl+V with files copied, on a line that names someone: they're sent as a drop would be (share.rs)
        if vk == 0x56
            && mods.ctrl
            && !mods.alt
            && !mods.shift
            && app.model.systems.as_ref().is_none_or(|s| s.focus.is_none())
            && crate::share::paste_files(app)
        {
            return true;
        }
        let plain = !mods.ctrl && !mods.alt && !mods.win;
        match vk {
            ESC if plain => {
                if messaging::escape(&mut app.model) {
                    return true;
                }
                if games_shown(&app.model) {
                    clear_line(app, false);
                    return true;
                }
                false
            }
            // the table never takes the keys, so while it has a formula ↵ puts that in
            ENTER if plain && !mods.shift => {
                if app.model.selected.is_none() && sheets::has_formula() {
                    return sheets::put_formula_in(app);
                }
                app.model.selected.is_none() && enter(app)
            }
            UP | DOWN if plain && !mods.shift => games_key(&mut app.model, vk == UP),
            _ => {
                // the walkthrough's copy step: the copy shortcut with an answer to copy
                if app.model.keybinds.action(mods, vk) == Some(crate::keys::KeyAction::CopyAnswer)
                    && !app.model.field.has_selection()
                    && app
                        .model
                        .live
                        .as_ref()
                        .is_some_and(|a| !a.display.is_empty())
                {
                    tour::copied(app);
                }
                false
            }
        }
    }

    /// WM_CHAR: a reply line's, while it has the keys.
    pub fn char16(app: &mut App, unit: u16) -> bool {
        messaging::char16(app, unit)
    }

    /// A click in the bar: the edge tools, the tour's skip, the cards, the games list. True when taken.
    pub fn click(app: &mut App, x: f32, y: f32) -> bool {
        crate::hover::key_down();
        crate::hover::triple_click(app, x, y)
            || crate::edge::click(app, x, y)
            || tour::click(app, x, y)
            || messaging::click(app, x, y)
            || click_game(app, x, y)
    }

    /// The pointer over the bar (None when it left): the edge tools, the cards' hold, the games list's hover.
    pub fn pointer(app: &mut App, at: Option<(f32, f32)>) {
        crate::edge::pointer(app, at);
        crate::hover::pointer(app, at);
        messaging::hover(app, at);
        let row = at.and_then(|(x, y)| game_at(&app.model, app.bar.layout.width, x, y));
        if row != app.model.games_hover {
            app.model.games_hover = row;
            app.model.touch();
        }
    }

    /// The bar hid: this package's hover and focus let go.
    pub fn will_hide(m: &mut BarModel) {
        crate::edge::will_hide(m);
        crate::hover::will_hide();
        messaging::will_hide(m);
        tour::will_hide(m);
        m.games_hover = None;
    }

    // ---- the answer slot ----

    /// How wide the answer slot wants to be for a command's words (None when there are none to show).
    pub fn slot_width(gfx: &Gfx, m: &BarModel) -> Option<f32> {
        if let Some(w) = crate::selection::slot_width(gfx, m) {
            return Some(w);
        }
        if m.command.is_none()
            && let Some(answer) = tour::example_answer(m)
        {
            let l = gfx.layout(answer, &gfx.fonts.display, 0.0, 36.0)?;
            return Some(measure(&l).0 + 2.0 * crate::answer::PAD_X + 1.0);
        }
        let words = &m.command.as_ref()?.message;
        if words.is_empty() {
            return Some(0.0);
        }
        let l = gfx.layout(words, &gfx.fonts.body, 0.0, 36.0)?;
        Some(measure(&l).0 + 2.0 * crate::answer::PAD_X + 1.0)
    }

    /// A command's words when the slot `r` cuts them at the end (the page's `.live.message` ellipsis): the whole
    /// text, for the tooltip (hover.rs). None when they fit or there's no command.
    pub fn slot_cut(gfx: &Gfx, m: &BarModel, r: Rect) -> Option<String> {
        if crate::selection::slot_width(gfx, m).is_some() {
            return None;
        }
        let words = &m.command.as_ref()?.message;
        if words.is_empty() {
            return None;
        }
        let l = gfx.layout(words, &gfx.fonts.body, 0.0, 36.0)?;
        (measure(&l).0 > r.w - 2.0 * crate::answer::PAD_X + 0.5).then(|| words.clone())
    }

    /// Paints a command's words in the answer slot; false when there's no command (the answer paints instead).
    pub fn paint_slot(c: &Canvas, m: &BarModel, r: Rect) -> bool {
        if crate::selection::paint_slot(c, m, r) {
            return true;
        }
        let Some(shown) = &m.command else {
            // the empty bar's example answer, muted at 70%
            if let Some(answer) = tour::example_answer(m) {
                let inner = r.inset(crate::answer::PAD_X, 0.0);
                if let Some(l) = c.gfx.layout_in(
                    answer,
                    &c.gfx.fonts.display,
                    inner.w,
                    inner.h,
                    DWRITE_TEXT_ALIGNMENT_TRAILING,
                ) {
                    c.text(&l, inner.x, inner.y, with_alpha(c.theme.muted, 0.7));
                }
                return true;
            }
            return false;
        };
        if shown.message.is_empty() || r.w <= 0.0 {
            return true;
        }
        let inner = r.inset(crate::answer::PAD_X, 0.0);
        if let Some(l) = c.gfx.layout_in(
            &shown.message,
            &c.gfx.fonts.body,
            inner.w,
            inner.h,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        ) {
            c.text(&l, inner.x, inner.y, c.theme.muted);
        }
        true
    }

    // ---- the games list above the bar ----

    /// The list's height above the composer (0 when it isn't up).
    pub fn games_height(m: &BarModel) -> f32 {
        if games_shown(m) {
            super::GAME_COUNT as f32 * GAME_ROW + 2.0 * crate::history::PAD_Y + 1.0
        } else {
            0.0
        }
    }

    fn game_row_rect(width: f32, i: usize) -> Rect {
        Rect::new(
            crate::history::PAD_X,
            crate::history::PAD_Y + i as f32 * GAME_ROW,
            width - 2.0 * crate::history::PAD_X,
            GAME_ROW,
        )
    }

    pub fn paint_games(c: &Canvas, m: &BarModel) {
        if !games_shown(m) {
            return;
        }
        let t = c.theme;
        let h = games_height(m);
        c.fill(Rect::new(0.0, 0.0, c.width, h), t.bg_tape);
        c.fill(Rect::new(0.0, h - 1.0, c.width, 1.0), t.line);
        for (i, (_, name, blurb)) in game_rows().enumerate() {
            let r = game_row_rect(c.width, i);
            if m.games_pick == Some(i) || m.games_hover == Some(i) {
                c.fill_round(r, size::RADIUS_S, t.fill_hover);
            }
            let inner = r.inset(8.0, 0.0);
            if let Some(l) = c.gfx.layout_in(
                name,
                &c.gfx.fonts.body,
                GAME_NAME_W,
                r.h,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ) {
                c.text(&l, inner.x, inner.y, t.ink);
            }
            let bx = inner.x + GAME_NAME_W + 12.0;
            if let Some(l) = c.gfx.layout_in(
                blurb,
                &c.gfx.fonts.body,
                (inner.right() - bx).max(1.0),
                r.h,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            ) {
                c.text(&l, bx, inner.y, with_alpha(t.muted, 1.0));
            }
        }
    }

    /// The games row under a point.
    pub fn game_at(m: &BarModel, width: f32, x: f32, y: f32) -> Option<usize> {
        if !games_shown(m) {
            return None;
        }
        (0..super::GAME_COUNT).find(|&i| game_row_rect(width, i).contains(x, y))
    }

    /// A click on a games row plays it.
    pub fn click_game(app: &mut App, x: f32, y: f32) -> bool {
        let Some(i) = game_at(&app.model, app.bar.layout.width, x, y) else {
            return false;
        };
        if let Some((id, ..)) = game_rows().nth(i) {
            play(app, id);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_read_as_the_mac_reads_them() {
        assert_eq!(parse("periodic"), Some(Command::Periodic));
        assert_eq!(parse("  Periodic   Table "), Some(Command::Periodic));
        match parse("idt summation") {
            Some(Command::Identities { id, section, hint }) => {
                assert_eq!(id, "identities");
                assert!(section > 0);
                assert!(hint.contains("summation"), "{hint}");
            }
            other => panic!("{other:?}"),
        }
        assert!(
            matches!(parse("trig"), Some(Command::Identities { id, .. }) if id == "identities")
        );
        assert!(
            matches!(parse("constants"), Some(Command::Identities { id, .. }) if id == "constants")
        );
        assert!(
            matches!(parse("conversion"), Some(Command::Identities { id, .. }) if id == "conversions")
        );
        assert!(
            matches!(parse("metric prefixes"), Some(Command::Identities { id, .. }) if id == "conversions")
        );
        assert!(
            matches!(parse("derivatives"), Some(Command::Identities { id, .. }) if id == "calculus")
        );
        assert_eq!(parse("games"), Some(Command::Games));
        assert_eq!(parse("Connect 4"), Some(Command::Game(GameKind::Connect4)));
        assert_eq!(
            parse("message max hello there"),
            Some(Command::Message {
                to: "max".into(),
                name: "max".into(),
                text: "hello there".into()
            })
        );
        assert_eq!(parse("tutorial"), Some(Command::Tutorial));
        assert_eq!(parse("skip"), Some(Command::Skip));
        assert_eq!(parse("hello!"), Some(Command::Greeting));
        assert_eq!(parse("Game of 24"), Some(Command::Game(GameKind::Numbers)));
        assert_eq!(
            parse("24 / countdown"),
            Some(Command::Game(GameKind::Numbers))
        );
        assert_eq!(parse("fire and ice"), Some(Command::Game(GameKind::Ember)));
        assert!(matches!(
            parse("physical constants"),
            Some(Command::Identities { .. })
        ));
        assert!(matches!(
            parse("si prefixes"),
            Some(Command::Identities { .. })
        ));
        assert!(matches!(
            parse("units table"),
            Some(Command::Identities { .. })
        ));
        assert!(matches!(
            parse("common integrals"),
            Some(Command::Identities { .. })
        ));
        assert!(matches!(
            parse("logarithm identities"),
            Some(Command::Identities { .. })
        ));
        assert_eq!(parse("howdy"), Some(Command::Greeting));
        assert_eq!(parse("tour"), Some(Command::Tutorial));
        // maths, and the other packages' commands, aren't ours
        for maths in [
            "",
            "2+2",
            "hi",
            "sin(30)",
            "5 ft to cm",
            "help",
            "?",
            "graph x^2",
            "settings",
        ] {
            assert_eq!(parse(maths), None, "{maths}");
        }
    }

    #[test]
    fn hints_say_what_enter_does() {
        let say = |c: &Command| hint(c, |_, name, _| format!("to {name}"));
        assert_eq!(say(&Command::Periodic), "periodic table");
        assert_eq!(say(&Command::Greeting), "hi 👋");
        assert_eq!(say(&Command::Game(GameKind::Pong)), "↵ play pong");
        assert_eq!(say(&Command::Tutorial), "start the tutorial");
        assert_eq!(say(&parse("message ann hi").unwrap()), "to ann");
    }

    #[test]
    fn the_games_list_walks_from_the_bottom() {
        assert_eq!(games_step(None, true), Some(GAME_COUNT - 1));
        assert_eq!(games_step(Some(0), true), Some(0));
        assert_eq!(games_step(Some(2), true), Some(1));
        assert_eq!(games_step(Some(GAME_COUNT - 1), false), None);
        assert_eq!(games_step(None, false), None);
        assert_eq!(game_rows().count(), GAME_COUNT);
        assert_eq!(game_rows().next().map(|r| r.0), Some("pong"));
    }
}

#[cfg(test)]
mod timing {
    /// `cargo test --release -p qcalc-win parse_cost -- --ignored --nocapture`: what reading a keystroke's line
    /// as a command costs.
    #[test]
    #[ignore]
    fn parse_cost() {
        let inputs = [
            "2+2",
            "sin(90)",
            "72 f to c",
            "1/3 + 1/6",
            "sqrt(2)*pi",
            "5 km in miles",
            "3x+2=11",
            "(1+2)*(3+4)/5^2",
            "20% of 80",
            "e^(i pi)+1",
        ];
        let mut prefixes = Vec::new();
        for i in inputs {
            for n in 1..=i.len() {
                if i.is_char_boundary(n) {
                    prefixes.push(&i[..n]);
                }
            }
        }
        for p in &prefixes {
            let _ = super::parse(p);
        }
        let t = std::time::Instant::now();
        let rounds = 20;
        for _ in 0..rounds {
            for p in &prefixes {
                std::hint::black_box(super::parse(p));
            }
        }
        let per = t.elapsed().as_secs_f64() * 1e6 / (rounds * prefixes.len()) as f64;
        println!(
            "parse: {per:.2} µs a keystroke over {} lines",
            prefixes.len()
        );
    }
}
