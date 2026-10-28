//! UI Automation for the windows Q Calc draws itself (the bar, settings): Narrator, NVDA and JAWS see a tree of
//! named parts instead of one blank window.
//!
//! A window answers WM_GETOBJECT(UiaRootObjectId) with `get_object`, giving a provider over a `Source`: a function
//! that describes the window as it is now (`Tree`, a flat list of `Node`s in screen pixels). Every call from a client
//! asks the source again, so nothing is kept in step by hand; a source that can't look (the app is busy in a
//! re-entrant message) gets the last tree it gave. The parts are the root's children, one level deep: an edit with
//! a value, live text (the answer, the hint), list items (the tape's rows), check boxes, radio buttons, combo boxes,
//! buttons, tabs, headings.
//!
//! `changed` raises what a reader speaks: focus moving to a part, a live part's new text (LiveRegionChanged), the
//! edit's new value, and `notify` a notification (UiaRaiseNotificationEvent) for news with no part of its own (a
//! mode toggled, a card). None of it runs, and uiautomationcore.dll isn't loaded, until a client has asked a window
//! for its provider (`provided`), so the app costs nothing more with no reader.
//!
//! The provider is not free-threaded by COM's rules (no ProviderOptions_UseComThreading, so the UI thread needs no
//! COM apartment): UIA calls it on a thread of its own. So a window's last tree is kept behind a lock (`WINDOWS`),
//! and a call from another thread first asks the window's own thread to look again (WM_GETOBJECT with `REFRESH`,
//! sent with a short timeout, at most every `FRESH_MS`); a busy or hung window answers with the tree it had.

// the windows crate names UIA's ids as the SDK does (UIA_NamePropertyId), and they are matched on here
#![allow(non_upper_case_globals)]

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_GETOBJECT,
};
use windows::core::{
    BOOL, BSTR, Error, IUnknown, IUnknownImpl, Interface, PCWSTR, Result, implement,
};

/// What a part is (its UIA control type and patterns).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Text,
    /// a heading's text (Narrator's H key)
    Heading,
    /// a line of text with a value (Value pattern, read-only to clients)
    Edit,
    ListItem,
    Button,
    /// with `toggled` (Toggle pattern)
    CheckBox,
    /// with `selected` (SelectionItem pattern)
    RadioButton,
    /// a list that opens; its `value` is the choice
    ComboBox,
    /// with `selected`: a page of a window
    Tab,
}

impl Role {
    fn control_type(self) -> UIA_CONTROLTYPE_ID {
        match self {
            Role::Text | Role::Heading => UIA_TextControlTypeId,
            Role::Edit => UIA_EditControlTypeId,
            Role::ListItem => UIA_ListItemControlTypeId,
            Role::Button => UIA_ButtonControlTypeId,
            Role::CheckBox => UIA_CheckBoxControlTypeId,
            Role::RadioButton => UIA_RadioButtonControlTypeId,
            Role::ComboBox => UIA_ComboBoxControlTypeId,
            Role::Tab => UIA_TabItemControlTypeId,
        }
    }
}

/// One part of a window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// stable while the part is the same part (the runtime id's last number; also its AutomationId)
    pub id: i32,
    pub role: Role,
    pub name: String,
    /// the Value pattern's value (an edit, a combo box)
    pub value: Option<String>,
    pub help: String,
    /// screen pixels: left, top, width, height
    pub rect: [f64; 4],
    pub focusable: bool,
    pub focused: bool,
    /// a live region (polite): a reader speaks its new name as it changes
    pub live: bool,
    pub toggled: Option<bool>,
    pub selected: Option<bool>,
}

/// A window's parts as they are now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub name: String,
    pub nodes: Vec<Node>,
}

impl Tree {
    fn node(&self, id: i32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

/// Describes a window now; None when it can't look (the last tree is used).
pub type Source = fn(HWND) -> Option<Tree>;

/// A window's source, its last tree and when it was taken.
struct Kept {
    source: Source,
    tree: Tree,
    at: Option<Instant>,
}

/// per window (HWND as isize): read from UIA's threads, written on the window's
static WINDOWS: Mutex<Option<HashMap<isize, Kept>>> = Mutex::new(None);

/// WM_GETOBJECT's lParam that asks the window's thread to look again (no OBJID uses it).
const REFRESH: isize = 0x5143_5541;
/// A tree this fresh is used as is by another thread's calls (a client asks many things at once).
const FRESH_MS: u128 = 40;
/// How long another thread waits for the window to look again.
const REFRESH_WAIT_MS: u32 = 250;

/// UIA_E_ELEMENTNOTAVAILABLE: the part is gone.
const GONE: windows::core::HRESULT = windows::core::HRESULT(0x8004_0201_u32 as i32);

fn with_kept<R>(f: impl FnOnce(&mut HashMap<isize, Kept>) -> R) -> R {
    let mut g = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

/// On the window's thread: the source's tree, kept; None when it can't look.
fn look(hwnd: isize, source: Source) -> Option<Tree> {
    let t = source(HWND(hwnd as *mut _))?;
    with_kept(|w| {
        w.insert(
            hwnd,
            Kept {
                source,
                tree: t.clone(),
                at: Some(Instant::now()),
            },
        )
    });
    Some(t)
}

fn kept(hwnd: isize) -> Tree {
    with_kept(|w| w.get(&hwnd).map(|k| k.tree.clone()).unwrap_or_default())
}

/// The window's tree now, from any thread.
fn tree_of(hwnd: isize, source: Source) -> Tree {
    let h = HWND(hwnd as *mut _);
    // SAFETY: plain queries
    let own = unsafe { GetWindowThreadProcessId(h, None) == GetCurrentThreadId() };
    if own {
        return look(hwnd, source).unwrap_or_else(|| kept(hwnd));
    }
    let fresh = with_kept(|w| {
        w.get(&hwnd)
            .and_then(|k| k.at)
            .is_some_and(|at| at.elapsed().as_millis() < FRESH_MS)
    });
    if !fresh {
        let mut out = 0usize;
        // SAFETY: our window; the reply is only waited for briefly
        let _ = unsafe {
            SendMessageTimeoutW(
                h,
                WM_GETOBJECT,
                WPARAM(0),
                LPARAM(REFRESH),
                SMTO_BLOCK | SMTO_ABORTIFHUNG,
                REFRESH_WAIT_MS,
                Some(&mut out),
            )
        };
    }
    kept(hwnd)
}

/// WM_GETOBJECT: our provider when UIA asks for it (lParam UiaRootObjectId), a fresh look when a provider asks
/// (`REFRESH`), else None (DefWindowProc answers).
pub fn get_object(hwnd: HWND, wp: WPARAM, lp: LPARAM, source: Source) -> Option<LRESULT> {
    let key = hwnd.0 as isize;
    if lp.0 == REFRESH {
        let _ = look(key, source);
        return Some(LRESULT(1));
    }
    if lp.0 as i32 != UiaRootObjectId {
        return None;
    }
    if look(key, source).is_none() {
        with_kept(|w| {
            w.entry(key).or_insert_with(|| Kept {
                source,
                tree: Tree::default(),
                at: None,
            });
        });
    }
    let root: IRawElementProviderSimple = Root { hwnd: key, source }.into();
    // SAFETY: our window, on its thread, answering its own WM_GETOBJECT
    Some(unsafe { UiaReturnRawElementProvider(hwnd, wp, lp, &root) })
}

/// Whether a client has asked this window for its provider (from then on its events are raised).
pub fn provided(hwnd: HWND) -> bool {
    with_kept(|w| w.contains_key(&(hwnd.0 as isize)))
}

/// The window is gone: its provider's parts answer "not available" from now on.
pub fn forget(hwnd: HWND) {
    let gone = with_kept(|w| w.remove(&(hwnd.0 as isize)));
    if gone.is_some() {
        // SAFETY: a plain call; our window is being destroyed
        let _ = unsafe { UiaReturnRawElementProvider(hwnd, WPARAM(0), LPARAM(0), None) };
    }
}

/// Whether any UIA client listens for events (Narrator does, without setting SPI_SETSCREENREADER).
pub fn listening() -> bool {
    // SAFETY: a plain query
    unsafe { UiaClientsAreListening().as_bool() }
}

/// What changed between two trees that a reader should hear, as (node id, event).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// the focus moved to this part
    Focus(i32),
    /// a live part's text changed (and isn't empty)
    Live(i32),
    /// an edit's value changed
    Value(i32),
}

/// The events from `before` to `now`.
pub fn changes(before: &Tree, now: &Tree) -> Vec<Change> {
    let mut out = Vec::new();
    for n in &now.nodes {
        let old = before.node(n.id);
        if n.focused && !old.is_some_and(|o| o.focused) {
            out.push(Change::Focus(n.id));
        }
        if n.live && !n.name.trim().is_empty() && old.is_none_or(|o| o.name != n.name) {
            out.push(Change::Live(n.id));
        }
        if n.value.is_some() && old.is_some_and(|o| o.value != n.value) {
            out.push(Change::Value(n.id));
        }
    }
    out
}

/// After a window's state may have changed (on its thread, outside its borrow): raises focus, live-region and
/// value events for what changed since the last look. `now` is the tree if the caller has it, else the source is
/// asked. Nothing unless a client has asked for this window.
pub fn changed(hwnd: HWND, now: Option<Tree>) {
    let key = hwnd.0 as isize;
    let Some((source, before)) = with_kept(|w| w.get(&key).map(|k| (k.source, k.tree.clone())))
    else {
        return;
    };
    let Some(now) = now.or_else(|| source(hwnd)) else {
        return;
    };
    if now == before {
        return;
    }
    with_kept(|w| {
        w.insert(
            key,
            Kept {
                source,
                tree: now.clone(),
                at: Some(Instant::now()),
            },
        )
    });
    if !listening() {
        return;
    }
    for c in changes(&before, &now) {
        let (id, event) = match c {
            Change::Focus(id) => (id, Some(UIA_AutomationFocusChangedEventId)),
            Change::Live(id) => (id, Some(UIA_LiveRegionChangedEventId)),
            Change::Value(id) => (id, None),
        };
        let child: IRawElementProviderSimple = Child {
            hwnd: key,
            source,
            id,
        }
        .into();
        // SAFETY: plain notifications about our own provider
        unsafe {
            match event {
                Some(e) => {
                    let _ = UiaRaiseAutomationEvent(&child, e);
                }
                None => {
                    let old = before
                        .node(id)
                        .and_then(|n| n.value.clone())
                        .unwrap_or_default();
                    let new = now
                        .node(id)
                        .and_then(|n| n.value.clone())
                        .unwrap_or_default();
                    let _ = UiaRaiseAutomationPropertyChangedEvent(
                        &child,
                        UIA_ValueValuePropertyId,
                        &VARIANT::from(old.as_str()),
                        &VARIANT::from(new.as_str()),
                    );
                }
            }
        }
    }
}

/// Has a reader say `text` (a notification from the window's root), e.g. a mode toggled or a card arriving.
pub fn notify(hwnd: HWND, text: &str) {
    let key = hwnd.0 as isize;
    let Some(source) = with_kept(|w| w.get(&key).map(|k| k.source)) else {
        return;
    };
    if text.trim().is_empty() || !listening() {
        return;
    }
    let root: IRawElementProviderSimple = Root { hwnd: key, source }.into();
    // SAFETY: a plain notification about our own provider
    let _ = unsafe {
        UiaRaiseNotificationEvent(
            &root,
            NotificationKind_Other,
            NotificationProcessing_ImportantAll,
            &BSTR::from(text.trim()),
            &BSTR::from("qcalc"),
        )
    };
}

fn runtime_id(id: i32) -> *mut SAFEARRAY {
    // SAFETY: a new vector of two ints, filled in place; UIA takes ownership
    unsafe {
        let sa = SafeArrayCreateVector(VT_I4, 0, 2);
        if sa.is_null() {
            return sa;
        }
        for (i, v) in [UiaAppendRuntimeId as i32, id].iter().enumerate() {
            let at = i as i32;
            let _ = SafeArrayPutElement(sa, &at, (v as *const i32).cast());
        }
        sa
    }
}

/// The window itself: a fragment root whose children are the tree's nodes. The host provider (from the HWND) gives
/// its bounds, runtime id and window properties; it adds the name and its parts.
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
struct Root {
    hwnd: isize,
    source: Source,
}

impl Root {
    fn child(&self, id: i32) -> IRawElementProviderFragment {
        Child {
            hwnd: self.hwnd,
            source: self.source,
            id,
        }
        .into()
    }
}

impl IRawElementProviderSimple_Impl for Root_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, _id: UIA_PATTERN_ID) -> Result<IUnknown> {
        Err(Error::empty())
    }

    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(match id {
            UIA_NamePropertyId => VARIANT::from(tree_of(self.hwnd, self.source).name.as_str()),
            UIA_ProviderDescriptionPropertyId => VARIANT::from("Q Calc"),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        // SAFETY: our window
        unsafe { UiaHostProviderFromHwnd(HWND(self.hwnd as *mut _)) }
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let t = tree_of(self.hwnd, self.source);
        let id = match direction {
            NavigateDirection_FirstChild => t.nodes.first().map(|n| n.id),
            NavigateDirection_LastChild => t.nodes.last().map(|n| n.id),
            _ => None,
        };
        id.map(|id| self.child(id)).ok_or_else(Error::empty)
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(UiaRect::default())
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.to_interface())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let t = tree_of(self.hwnd, self.source);
        t.nodes
            .iter()
            .rev()
            .find(|n| {
                let [l, top, w, h] = n.rect;
                x >= l && x < l + w && y >= top && y < top + h
            })
            .map(|n| self.child(n.id))
            .ok_or_else(Error::empty)
    }

    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let t = tree_of(self.hwnd, self.source);
        t.nodes
            .iter()
            .find(|n| n.focused)
            .map(|n| self.child(n.id))
            .ok_or_else(Error::empty)
    }
}

/// One part, found by id in the window's tree each time it's asked.
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IValueProvider,
    IToggleProvider,
    ISelectionItemProvider
)]
struct Child {
    hwnd: isize,
    source: Source,
    id: i32,
}

impl Child {
    fn node(&self) -> Result<Node> {
        tree_of(self.hwnd, self.source)
            .node(self.id)
            .cloned()
            .ok_or_else(|| Error::from(GONE))
    }
}

impl IRawElementProviderSimple_Impl for Child_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, id: UIA_PATTERN_ID) -> Result<IUnknown> {
        let n = self.node()?;
        let has = match id {
            UIA_ValuePatternId => n.value.is_some(),
            UIA_TogglePatternId => n.toggled.is_some(),
            UIA_SelectionItemPatternId => n.selected.is_some(),
            _ => false,
        };
        if has {
            Ok(self.to_interface::<IRawElementProviderSimple>().cast()?)
        } else {
            Err(Error::empty())
        }
    }

    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let n = self.node()?;
        Ok(match id {
            UIA_ControlTypePropertyId => VARIANT::from(n.role.control_type().0),
            UIA_NamePropertyId => VARIANT::from(n.name.as_str()),
            UIA_HelpTextPropertyId if !n.help.is_empty() => VARIANT::from(n.help.as_str()),
            UIA_AutomationIdPropertyId => VARIANT::from(format!("qcalc-{}", n.id).as_str()),
            UIA_IsKeyboardFocusablePropertyId => VARIANT::from(n.focusable),
            UIA_HasKeyboardFocusPropertyId => VARIANT::from(n.focused),
            UIA_IsEnabledPropertyId | UIA_IsContentElementPropertyId => VARIANT::from(true),
            UIA_IsControlElementPropertyId => VARIANT::from(true),
            UIA_IsOffscreenPropertyId => VARIANT::from(n.rect[2] <= 0.0 || n.rect[3] <= 0.0),
            UIA_LiveSettingPropertyId if n.live => VARIANT::from(Polite.0),
            UIA_HeadingLevelPropertyId if n.role == Role::Heading => VARIANT::from(HeadingLevel2.0),
            UIA_FrameworkIdPropertyId => VARIANT::from("QCalc"),
            _ => VARIANT::default(),
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        Err(Error::empty())
    }
}

impl IRawElementProviderFragment_Impl for Child_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        if direction == NavigateDirection_Parent {
            let root: IRawElementProviderFragment = Root {
                hwnd: self.hwnd,
                source: self.source,
            }
            .into();
            return Ok(root);
        }
        let t = tree_of(self.hwnd, self.source);
        let Some(i) = t.nodes.iter().position(|n| n.id == self.id) else {
            return Err(Error::from(GONE));
        };
        let next = match direction {
            NavigateDirection_NextSibling => t.nodes.get(i + 1),
            NavigateDirection_PreviousSibling => i.checked_sub(1).and_then(|j| t.nodes.get(j)),
            _ => None,
        };
        next.map(|n| {
            Child {
                hwnd: self.hwnd,
                source: self.source,
                id: n.id,
            }
            .into()
        })
        .ok_or_else(Error::empty)
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(runtime_id(self.id))
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let [left, top, width, height] = self.node()?.rect;
        Ok(UiaRect {
            left,
            top,
            width,
            height,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        // the window keeps its own focus; a part can't take it from a client
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(Root {
            hwnd: self.hwnd,
            source: self.source,
        }
        .into())
    }
}

impl IValueProvider_Impl for Child_Impl {
    fn SetValue(&self, _val: &PCWSTR) -> Result<()> {
        Err(Error::from(E_FAIL))
    }

    fn Value(&self) -> Result<BSTR> {
        Ok(BSTR::from(self.node()?.value.unwrap_or_default()))
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        // typed into with the keyboard, not set by a client
        Ok(BOOL::from(true))
    }
}

impl IToggleProvider_Impl for Child_Impl {
    fn Toggle(&self) -> Result<()> {
        Err(Error::from(E_FAIL))
    }

    fn ToggleState(&self) -> Result<ToggleState> {
        Ok(if self.node()?.toggled == Some(true) {
            ToggleState_On
        } else {
            ToggleState_Off
        })
    }
}

impl ISelectionItemProvider_Impl for Child_Impl {
    fn Select(&self) -> Result<()> {
        Err(Error::from(E_FAIL))
    }

    fn AddToSelection(&self) -> Result<()> {
        Err(Error::from(E_FAIL))
    }

    fn RemoveFromSelection(&self) -> Result<()> {
        Err(Error::from(E_FAIL))
    }

    fn IsSelected(&self) -> Result<BOOL> {
        Ok(BOOL::from(self.node()?.selected == Some(true)))
    }

    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        Err(Error::empty())
    }
}

/// Whether a reader may be listening to this window: a client has asked for its provider and one listens for
/// events (a cheap check, after the first).
pub fn heard(hwnd: HWND) -> bool {
    provided(hwnd) && listening()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i32, name: &str) -> Node {
        Node {
            id,
            name: name.into(),
            ..Node::default()
        }
    }

    #[test]
    fn what_a_reader_is_told() {
        let edit = |v: &str, focused| Node {
            role: Role::Edit,
            value: Some(v.into()),
            focused,
            ..node(1, "Expression")
        };
        let answer = |a: &str| Node {
            live: true,
            ..node(2, a)
        };
        let a = Tree {
            name: "Q Calc".into(),
            nodes: vec![edit("2+", false), answer("")],
        };
        let b = Tree {
            name: "Q Calc".into(),
            nodes: vec![edit("2+2", true), answer("4")],
        };
        assert_eq!(
            changes(&a, &b),
            vec![Change::Focus(1), Change::Value(1), Change::Live(2)]
        );
        // nothing new, nothing said; an answer gone blank isn't read out
        assert!(changes(&b, &b).is_empty());
        let c = Tree {
            nodes: vec![edit("2+2", true), answer(" ")],
            ..b.clone()
        };
        assert!(changes(&b, &c).is_empty());
        // a part that just appeared: its focus and live text, not a value change
        let d = Tree {
            nodes: vec![],
            ..b.clone()
        };
        assert_eq!(changes(&d, &b), vec![Change::Focus(1), Change::Live(2)]);
    }
}
