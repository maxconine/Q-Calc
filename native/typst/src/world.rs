//! The in-memory Typst world: the preview document at `/main.typ`, the macros at `/macros.typ`, the three New
//! Computer Modern fonts the web preview loads, and nothing else (no files, packages, clock or system fonts).

use std::sync::{Arc, Mutex, Weak};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// The macros `typstDocument` imports (`TYPST_PREAMBLE` in `src/lib/typstMath.ts`, kept verbatim; a test checks
/// that the two stay equal).
pub const MACROS: &str = include_str!("macros.typ");

/// The fonts the web preview loads (`FONT_FILES` in `src/lib/typstRender.ts`), from `public/typst/`, so both
/// previews shape with the same files. `loadFonts(..., { assets: false })` means typst.ts adds no fonts of its
/// own, so these three are the whole font book there too.
const FONT_FILES: [&[u8]; 3] = [
    include_bytes!("../../../public/typst/NewCMMath-Regular.otf"),
    include_bytes!("../../../public/typst/NewCM10-Regular.otf"),
    include_bytes!("../../../public/typst/NewCM10-Bold.otf"),
];

/// The licence the fonts ship under (SIL OFL 1.1), for the app's acknowledgements.
pub const FONT_NOTICE: &str = include_str!("../../../public/typst/NOTICE.txt");

/// Fonts, font book and standard library, built once and shared by every world while any world lives: an app
/// that keeps a renderer (the Mac's) builds them once; one that drops its renderer while idle (Windows, hidden)
/// gets their memory back and builds them again with the next renderer.
struct Shared {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn shared() -> Arc<Shared> {
    static SHARED: Mutex<Weak<Shared>> = Mutex::new(Weak::new());
    let mut slot = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = slot.upgrade() {
        return s;
    }
    let fonts: Vec<Font> = FONT_FILES
        .iter()
        .flat_map(|data| Font::iter(Bytes::new(*data)))
        .collect();
    let s = Arc::new(Shared {
        library: LazyHash::new(Library::default()),
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
    });
    *slot = Arc::downgrade(&s);
    s
}

/// How many font faces the world has (three: math, regular, bold).
pub fn font_count() -> usize {
    shared().fonts.len()
}

pub(crate) struct PreviewWorld {
    main: Source,
    macros: Source,
    shared: Arc<Shared>,
}

impl PreviewWorld {
    pub(crate) fn new(macros: &str) -> Self {
        let main_id = FileId::new(None, VirtualPath::new("/main.typ"));
        let macros_id = FileId::new(None, VirtualPath::new("/macros.typ"));
        // the shared state now, so constructing a world is where the one-time cost lands
        Self {
            main: Source::new(main_id, String::new()),
            macros: Source::new(macros_id, macros.to_string()),
            shared: shared(),
        }
    }

    /// Replace the main document. `Source::replace` reparses incrementally, and comemo reuses everything the
    /// edit didn't touch, so the next compile after one keystroke only redoes what changed.
    pub(crate) fn set_main(&mut self, text: &str) {
        if self.main.text() != text {
            self.main.replace(text);
        }
    }

    fn lookup(&self, id: FileId) -> FileResult<&Source> {
        if id == self.main.id() {
            Ok(&self.main)
        } else if id == self.macros.id() {
            Ok(&self.macros)
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }
}

impl World for PreviewWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.shared.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.shared.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.lookup(id).cloned()
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.lookup(id)
            .map(|source| Bytes::from_string(source.text().to_string()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.shared.fonts.get(index).cloned()
    }

    // the preview never shows a date; `datetime.today()` failing keeps every render deterministic
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}
