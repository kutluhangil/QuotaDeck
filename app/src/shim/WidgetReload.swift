// The one thing the host binary needs from WidgetKit.
//
// `WidgetCenter` is a Swift-only API with no Objective-C surface, so the Rust side cannot reach
// it through the runtime the way it reaches AppKit. `@_cdecl` gives it a C symbol instead, and
// the Swift toolchain is already a requirement of the extension this exists to reload.

import WidgetKit

@_cdecl("quotadeck_reload_widget")
public func quotadeckReloadWidget() {
    WidgetCenter.shared.reloadTimelines(ofKind: "QuotaDeckWidget")
}
