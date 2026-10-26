//! `hello` gets a hi back, where an answer would go (`src/lib/greeting.ts`). Not `hi` or `hey`, which could be
//! someone's variable.

use crate::jsre::{self, Lazy, js};

pub const GREETING_REPLY: &str = "hi 👋";

pub fn is_greeting(text: &str) -> bool {
    static GREETING: Lazy = Lazy::new(|| js(r"^(?:hello|hello there|hiya|howdy)\s*[!.]*$", "i"));
    jsre::test(&GREETING, crate::app::js::trim(text))
}
