//! What a screen reader hears from Q Calc: listens to UI Automation on one process's windows for a while and prints
//! each notification, live-region change, focus change and value change, one line each (uia.rs raises them).
//!
//!   cargo run --release -p qcalc-win --example uia_watch -- <pid> [seconds]
//!
//! The e2e suite (smoke.ps1 A1) runs it beside a copy started with QCALC_READER=1, which raises the events as with a
//! reader on, without starting one.

#[cfg(windows)]
fn main() {
    watch::main();
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
mod watch {
    use std::time::{Duration, Instant};

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::Accessibility::*;
    use windows::core::{BSTR, Ref, Result, implement};

    fn describe(e: &IUIAutomationElement) -> String {
        // SAFETY: plain property reads on a live element
        unsafe {
            let name = e.CurrentName().map(|b| b.to_string()).unwrap_or_default();
            let kind = e
                .CurrentLocalizedControlType()
                .map(|b| b.to_string())
                .unwrap_or_default();
            format!("{kind} '{name}'")
        }
    }

    fn pid_of(e: &IUIAutomationElement) -> i32 {
        // SAFETY: a plain property read
        unsafe { e.CurrentProcessId().unwrap_or(0) }
    }

    #[implement(
        IUIAutomationNotificationEventHandler,
        IUIAutomationEventHandler,
        IUIAutomationFocusChangedEventHandler,
        IUIAutomationPropertyChangedEventHandler
    )]
    struct Handler {
        pid: i32,
    }

    impl IUIAutomationNotificationEventHandler_Impl for Handler_Impl {
        fn HandleNotificationEvent(
            &self,
            sender: Ref<'_, IUIAutomationElement>,
            _kind: NotificationKind,
            _processing: NotificationProcessing,
            text: &BSTR,
            activity: &BSTR,
        ) -> Result<()> {
            if let Some(e) = sender.as_ref()
                && pid_of(e) == self.pid
            {
                println!("notification '{text}' ({activity}) from {}", describe(e));
            }
            Ok(())
        }
    }

    impl IUIAutomationEventHandler_Impl for Handler_Impl {
        fn HandleAutomationEvent(
            &self,
            sender: Ref<'_, IUIAutomationElement>,
            id: UIA_EVENT_ID,
        ) -> Result<()> {
            if let Some(e) = sender.as_ref()
                && pid_of(e) == self.pid
            {
                let what = if id == UIA_LiveRegionChangedEventId {
                    "live"
                } else {
                    "event"
                };
                println!("{what} {}", describe(e));
            }
            Ok(())
        }
    }

    impl IUIAutomationFocusChangedEventHandler_Impl for Handler_Impl {
        fn HandleFocusChangedEvent(&self, sender: Ref<'_, IUIAutomationElement>) -> Result<()> {
            if let Some(e) = sender.as_ref()
                && pid_of(e) == self.pid
            {
                println!("focus {}", describe(e));
            }
            Ok(())
        }
    }

    impl IUIAutomationPropertyChangedEventHandler_Impl for Handler_Impl {
        fn HandlePropertyChangedEvent(
            &self,
            sender: Ref<'_, IUIAutomationElement>,
            _id: UIA_PROPERTY_ID,
            value: &windows::Win32::System::Variant::VARIANT,
        ) -> Result<()> {
            if let Some(e) = sender.as_ref()
                && pid_of(e) == self.pid
            {
                println!("value '{value}' on {}", describe(e));
            }
            Ok(())
        }
    }

    pub fn main() {
        let args: Vec<String> = std::env::args().collect();
        let pid: i32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(0);
        let secs: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(10);
        // SAFETY: COM set up on this thread, then plain UIA client calls
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).expect("UIA");
            let root = uia.GetRootElement().expect("root");
            let handler = Handler { pid };
            let n: IUIAutomationNotificationEventHandler = handler.into();
            let a: IUIAutomationEventHandler = n.cast().expect("cast");
            let f: IUIAutomationFocusChangedEventHandler = n.cast().expect("cast");
            let p: IUIAutomationPropertyChangedEventHandler = n.cast().expect("cast");
            let uia5: IUIAutomation5 = uia.cast().expect("UIA 5");
            uia5.AddNotificationEventHandler(&root, TreeScope_Subtree, None, &n)
                .expect("notifications");
            uia.AddAutomationEventHandler(
                UIA_LiveRegionChangedEventId,
                &root,
                TreeScope_Subtree,
                None,
                &a,
            )
            .expect("live regions");
            uia.AddFocusChangedEventHandler(None, &f).expect("focus");
            uia.AddPropertyChangedEventHandlerNativeArray(
                &root,
                TreeScope_Subtree,
                None,
                &p,
                &[UIA_ValueValuePropertyId],
            )
            .expect("values");
            println!("watching pid {pid} for {secs} s");
            let end = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < end {
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = uia.RemoveAllEventHandlers();
        }
    }

    use windows::core::Interface;
}
