//! macOS 26 inserts `NSScrollPocket` over the traffic-light buttons. They
//! still paint, but hit testing lands on the pocket, so close / zoom /
//! minimize do nothing. Put the buttons above the pocket and make the pocket
//! ignore hits. AppKit inserts the pocket again after appearance changes, so
//! the UI pump reapplies this.

use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicU32, Ordering};

use objc2::ffi::{class_addMethod, class_getInstanceMethod, method_setImplementation};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::sel;
use objc2_app_kit::{NSApplication, NSView};
use objc2_foundation::{MainThreadMarker, NSPoint};

const NS_WINDOW_ABOVE: isize = 1;
static ROUTE_TICKS: AtomicU32 = AtomicU32::new(0);

extern "C-unwind" fn ignore_hit_test(
    _this: *mut AnyObject,
    _cmd: Sel,
    _point: NSPoint,
) -> *mut AnyObject {
    std::ptr::null_mut()
}

pub fn route_window_clicks() {
    let tick = ROUTE_TICKS.fetch_add(1, Ordering::Relaxed);
    // AppKit inserts the scroll pocket just after the window appears, and again
    // after appearance changes. Cover the first few seconds, then re-check
    // occasionally so we do not reorder views on every pump.
    if tick > 40 && tick % 20 != 0 {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    for window in app.windows().iter() {
        if !window.title().to_string().starts_with("ThroneRs") {
            continue;
        }
        window.setIgnoresMouseEvents(false);
        let Some(content) = window.contentView() else {
            continue;
        };
        let Some(theme) = (unsafe { content.superview() }) else {
            continue;
        };
        pass_through_pockets(&theme);
        raise_named(&theme, &["GPUIView", "Button", "Widget"]);
    }
}

fn pass_through_pockets(view: &NSView) {
    let name = class_name(view);
    if name.contains("Pocket") || name.contains("ScrollEdge") || name.contains("DecorationView") {
        force_hit_test_nil(view);
    }
    for sub in view.subviews().iter() {
        pass_through_pockets(&sub);
    }
}

fn force_hit_test_nil(view: &NSView) {
    let class = view_class(view) as *const AnyClass as *mut AnyClass;
    if class.is_null() {
        return;
    }
    let sel = sel!(hitTest:);
    let encoding = CString::new("@@:{CGPoint=dd}").expect("encoding");
    let imp: Imp = unsafe { std::mem::transmute(ignore_hit_test as *const () as usize) };
    let added = unsafe { class_addMethod(class, sel, imp, encoding.as_ptr()) };
    if added.as_bool() {
        return;
    }
    let method = unsafe { class_getInstanceMethod(class, sel) };
    if !method.is_null() {
        unsafe { method_setImplementation(method, imp) };
    }
}

fn raise_named(view: &NSView, needles: &[&str]) {
    let children: Vec<_> = view.subviews().iter().collect();
    for child in &children {
        raise_named(child, needles);
    }
    let name = class_name(view);
    if !needles.iter().any(|needle| name.contains(needle)) {
        return;
    }
    let Some(parent) = (unsafe { view.superview() }) else {
        return;
    };
    if parent
        .subviews()
        .iter()
        .last()
        .is_some_and(|front| std::ptr::eq(&*front as *const NSView, view as *const NSView))
    {
        return;
    }
    unsafe {
        let _: () = msg_send![
            &*parent,
            addSubview: view,
            positioned: NS_WINDOW_ABOVE,
            relativeTo: std::ptr::null::<NSView>()
        ];
    }
}

fn class_name(view: &NSView) -> String {
    let class = view_class(view);
    if class.is_null() {
        return String::new();
    }
    unsafe {
        CStr::from_ptr(objc2::ffi::class_getName(class))
            .to_string_lossy()
            .into_owned()
    }
}

fn view_class(view: &NSView) -> *const AnyClass {
    let object = view as *const NSView as *const AnyObject;
    unsafe { objc2::ffi::object_getClass(object) }
}
