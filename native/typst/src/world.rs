//! The in-memory Typst world: the preview document at `/main.typ`, the macros at `/macros.typ`, the three New
//! Computer Modern fonts the web preview loads, and nothing else (no files, packages, clock or system fonts).

use std::sync::OnceLock;

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

/// Fonts, font book and standard library, built once per process and shared by every world.
struct Shared {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let fonts: Vec<Font> = FONT_FILES
            .iter()
            .flat_map(|data| Font::iter(Bytes::new(*data)))
            .collect();
        Shared {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

/// How many font faces the world has (three: math, regular, bold).
pub fn font_count() -> usize {
    shared().fonts.len()
}

pub(crate) struct PreviewWorld {
    main: Source,
    macros: Source,
}

impl PreviewWorld {
    pub(crate) fn new(macros: &str) -> Self {
        let main_id = FileId::new(None, VirtualPath::new("/main.typ"));
        let macros_id = FileId::new(None, VirtualPath::new("/macros.typ"));
        // touch the shared state now, so constructing a world is where the one-time cost lands
        let _ = shared();
        Self {
            main: Source::new(main_id, String::new()),
            macros: Source::new(macros_id, macros.to_string()),
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
        &shared().library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &shared().book
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
        shared().fonts.get(index).cloned()
    }

    // the preview never shows a date; `datetime.today()` failing keeps every render deterministic
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}
