// Quota Deck fan helper.
//
// One binary, four roles:
//   daemon                 the root LaunchDaemon. Listens on the mach service, writes fan keys.
//   client <command> ...   what the app runs to talk to the daemon; prints one JSON line.
//   register | unregister | status
//                          SMAppService, run unprivileged from inside the app bundle.
//   selftest               layout and policy checks; never touches the SMC.
//
// Red lines (CLAUDE.md, v2 exceptions): only `F<n>md`, `F<n>Tg` and `Ftst` are ever written;
// every fan goes back to Auto when the app stops renewing its lease, on sleep, and on exit.

import Foundation
import IOKit
import IOKit.pwr_mgt
import ServiceManagement
import XPC

let label = "com.kutluhangil.quotadeck.fanhelper"
// Replaced by scripts/build-fanhelper.sh: the release build pins our Developer ID team, a dev
// build pins only the identifier. Left as is, the requirement is invalid and the daemon refuses
// to start, so an unconfigured build can never accept an arbitrary client.
let peerRequirement = #"@PEER_REQUIREMENT@"#

/// A forced fan is put back to Auto when the app has not renewed in this long.
let leaseSeconds: TimeInterval = 90

// MARK: - Policy (pure)

enum Mode: String { case auto, cool, max }

/// Target speed for a mode. Cool is 60 % of the maximum, never below the minimum.
func target(_ mode: Mode, min: Double, max: Double) -> Double? {
    switch mode {
    case .auto: return nil
    case .cool: return Swift.max(min, (max * 0.6).rounded())
    case .max: return max
    }
}

/// The only keys this helper may write.
func writable(_ key: String) -> Bool {
    if key == "Ftst" { return true }
    let chars = Array(key)
    guard chars.count == 4, chars[0] == "F", chars[1].isNumber else { return false }
    let suffix = String(chars[2...])
    return suffix == "md" || suffix == "Tg"
}

// MARK: - SMC

typealias Bytes32 = (
    UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
    UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
    UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
    UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8
)
let zero32: Bytes32 = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)

struct SMCVersion { var major: UInt8 = 0; var minor: UInt8 = 0; var build: UInt8 = 0; var reserved: UInt8 = 0; var release: UInt16 = 0 }
struct SMCLimit { var version: UInt16 = 0; var length: UInt16 = 0; var cpu: UInt32 = 0; var gpu: UInt32 = 0; var mem: UInt32 = 0 }
/// Swift lays the next field out after `size`, not `stride`, so C's trailing padding is explicit.
struct SMCKeyInfo { var size: UInt32 = 0; var type: UInt32 = 0; var attributes: UInt8 = 0; var padding: (UInt8, UInt8, UInt8) = (0, 0, 0) }
/// Mirrors the kernel's `SMCParamStruct`; must be exactly 80 bytes (checked by `selftest`).
struct SMCParam {
    var key: UInt32 = 0
    var vers = SMCVersion()
    var limit = SMCLimit()
    var info = SMCKeyInfo()
    var result: UInt8 = 0
    var status: UInt8 = 0
    var command: UInt8 = 0
    var index: UInt32 = 0
    var bytes: Bytes32 = zero32
}

func fourCC(_ key: String) -> UInt32 {
    key.utf8.reduce(0) { ($0 << 8) | UInt32($1) }
}

final class SMC {
    private var conn: io_connect_t = 0

    init?() {
        var iterator: io_iterator_t = 0
        guard IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("AppleSMC"), &iterator) == KERN_SUCCESS else { return nil }
        defer { IOObjectRelease(iterator) }
        var chosen: io_object_t = 0
        while case let service = IOIteratorNext(iterator), service != 0 {
            var name = [CChar](repeating: 0, count: 128)
            IORegistryEntryGetName(service, &name)
            if chosen == 0 || String(cString: name) == "AppleSMCKeysEndpoint" {
                if chosen != 0 { IOObjectRelease(chosen) }
                chosen = service
            } else {
                IOObjectRelease(service)
            }
        }
        guard chosen != 0 else { return nil }
        defer { IOObjectRelease(chosen) }
        guard IOServiceOpen(chosen, mach_task_self_, 0, &conn) == KERN_SUCCESS else { return nil }
    }

    deinit { IOServiceClose(conn) }

    private func call(_ input: SMCParam) -> SMCParam? {
        var input = input
        var output = SMCParam()
        var size = MemoryLayout<SMCParam>.stride
        let status = IOConnectCallStructMethod(conn, 2, &input, MemoryLayout<SMCParam>.stride, &output, &size)
        return status == KERN_SUCCESS && output.result == 0 ? output : nil
    }

    func info(_ key: String) -> SMCKeyInfo? {
        var param = SMCParam()
        param.key = fourCC(key)
        param.command = 9
        return call(param)?.info
    }

    func read(_ key: String) -> Double? {
        guard let info = info(key) else { return nil }
        var param = SMCParam()
        param.key = fourCC(key)
        param.command = 5
        param.info.size = info.size
        guard let out = call(param) else { return nil }
        let b = withUnsafeBytes(of: out.bytes) { Array($0) }
        switch info.type {
        case fourCC("flt "): return Double(Float(bitPattern: UInt32(b[0]) | UInt32(b[1]) << 8 | UInt32(b[2]) << 16 | UInt32(b[3]) << 24))
        case fourCC("ui8 "): return Double(b[0])
        default: return nil
        }
    }

    /// Writes one allowed key. Refuses everything else, whatever the caller asks.
    @discardableResult
    func write(_ key: String, _ value: Double) -> Bool {
        guard writable(key), let info = info(key) else { return false }
        var param = SMCParam()
        param.key = fourCC(key)
        param.command = 6
        param.info.size = info.size
        var raw = [UInt8](repeating: 0, count: 32)
        switch info.type {
        case fourCC("flt "):
            let bits = Float(value).bitPattern
            raw[0] = UInt8(bits & 0xff); raw[1] = UInt8(bits >> 8 & 0xff); raw[2] = UInt8(bits >> 16 & 0xff); raw[3] = UInt8(bits >> 24)
        case fourCC("ui8 "):
            raw[0] = UInt8(clamping: Int(value))
        default:
            return false
        }
        withUnsafeMutableBytes(of: &param.bytes) { buffer in
            for i in 0..<32 { buffer[i] = raw[i] }
        }
        return call(param) != nil
    }
}

// MARK: - Daemon

final class Fans {
    let smc: SMC
    var forced = Set<Int>()
    var lastRenewal = Date()

    init(smc: SMC) { self.smc = smc }

    var count: Int { Int(smc.read("FNum") ?? 0) }

    func set(_ fan: Int, _ mode: Mode) -> String? {
        guard fan >= 0, fan < count else { return "no such fan" }
        if mode == .auto {
            smc.write("F\(fan)md", 0)
            forced.remove(fan)
            if forced.isEmpty, smc.info("Ftst") != nil { smc.write("Ftst", 0) }
            return nil
        }
        guard let min = smc.read("F\(fan)Mn"), let max = smc.read("F\(fan)Mx"),
              let speed = target(mode, min: min, max: max) else { return "fan limits unreadable" }
        if smc.info("Ftst") != nil { smc.write("Ftst", 1) }
        guard smc.write("F\(fan)md", 1), smc.write("F\(fan)Tg", speed) else {
            smc.write("F\(fan)md", 0)
            return "the SMC refused the write"
        }
        forced.insert(fan)
        lastRenewal = Date()
        return nil
    }

    func restoreAll() {
        for fan in 0..<Swift.max(count, 1) { smc.write("F\(fan)md", 0) }
        if smc.info("Ftst") != nil { smc.write("Ftst", 0) }
        forced.removeAll()
    }

    func status() -> xpc_object_t {
        let list = xpc_array_create(nil, 0)
        for fan in 0..<count {
            let entry = xpc_dictionary_create(nil, nil, 0)
            xpc_dictionary_set_int64(entry, "index", Int64(fan))
            xpc_dictionary_set_bool(entry, "forced", forced.contains(fan))
            xpc_dictionary_set_double(entry, "rpm", smc.read("F\(fan)Ac") ?? 0)
            xpc_array_append_value(list, entry)
        }
        return list
    }
}

var fans: Fans?
var powerPort: io_connect_t = 0

// `iokit_common_msg(0x270)` and `(0x280)`: the C macros do not import into Swift.
let messageCanSystemSleep: UInt32 = 0xE000_0270
let messageSystemWillSleep: UInt32 = 0xE000_0280

func runDaemon() -> Never {
    guard let smc = SMC() else {
        FileHandle.standardError.write("quotadeck-fanhelper: AppleSMC could not be opened\n".data(using: .utf8)!)
        exit(1)
    }
    let state = Fans(smc: smc)
    fans = state
    let queue = DispatchQueue.main

    let listener = xpc_connection_create_mach_service(label, queue, UInt64(XPC_CONNECTION_MACH_SERVICE_LISTENER))
    // Without the requirement any local process could drive the fans: refuse to run.
    guard xpc_connection_set_peer_code_signing_requirement(listener, peerRequirement) == 0 else {
        FileHandle.standardError.write("quotadeck-fanhelper: peer requirement rejected\n".data(using: .utf8)!)
        exit(1)
    }
    xpc_connection_set_event_handler(listener) { peer in
        guard xpc_get_type(peer) == XPC_TYPE_CONNECTION else { return }
        xpc_connection_set_event_handler(peer) { message in
            guard xpc_get_type(message) == XPC_TYPE_DICTIONARY, let reply = xpc_dictionary_create_reply(message) else { return }
            let command = xpc_dictionary_get_string(message, "command").map { String(cString: $0) } ?? ""
            var error: String?
            switch command {
            case "set":
                let fan = Int(xpc_dictionary_get_int64(message, "fan"))
                let raw = xpc_dictionary_get_string(message, "mode").map { String(cString: $0) } ?? ""
                if let mode = Mode(rawValue: raw) { error = state.set(fan, mode) } else { error = "unknown mode" }
            case "renew":
                state.lastRenewal = Date()
            case "restore":
                state.restoreAll()
            case "status":
                break
            default:
                error = "unknown command"
            }
            xpc_dictionary_set_bool(reply, "ok", error == nil)
            if let error { xpc_dictionary_set_string(reply, "error", error) }
            xpc_dictionary_set_value(reply, "fans", state.status())
            xpc_connection_send_message(peer, reply)
        }
        xpc_connection_resume(peer)
    }
    xpc_connection_resume(listener)

    // Lease: an app that quit, crashed or hung stops renewing, and the fans go back to Auto.
    let lease = DispatchSource.makeTimerSource(queue: queue)
    lease.schedule(deadline: .now() + 15, repeating: 15)
    lease.setEventHandler {
        if !state.forced.isEmpty, Date().timeIntervalSince(state.lastRenewal) > leaseSeconds { state.restoreAll() }
    }
    lease.resume()

    // Stopped by launchd or `unregister`: Auto first.
    signal(SIGTERM, SIG_IGN)
    let term = DispatchSource.makeSignalSource(signal: SIGTERM, queue: queue)
    term.setEventHandler {
        state.restoreAll()
        exit(0)
    }
    term.resume()

    // Sleep: Auto before the machine goes down; the app sets the mode again after wake.
    var notifier: io_object_t = 0
    var portRef: IONotificationPortRef?
    powerPort = IORegisterForSystemPower(nil, &portRef, { _, _, messageType, argument in
        if messageType == messageSystemWillSleep || messageType == messageCanSystemSleep {
            fans?.restoreAll()
            IOAllowPowerChange(powerPort, Int(bitPattern: argument))
        }
    }, &notifier)
    if let portRef { IONotificationPortSetDispatchQueue(portRef, queue) }

    dispatchMain()
}

// MARK: - Client

func runClient(_ args: [String]) -> Int32 {
    let connection = xpc_connection_create_mach_service(label, nil, UInt64(XPC_CONNECTION_MACH_SERVICE_PRIVILEGED))
    xpc_connection_set_event_handler(connection) { _ in }
    xpc_connection_resume(connection)
    let message = xpc_dictionary_create(nil, nil, 0)
    xpc_dictionary_set_string(message, "command", args.first ?? "status")
    if args.first == "set", args.count == 3, let fan = Int64(args[1]) {
        xpc_dictionary_set_int64(message, "fan", fan)
        xpc_dictionary_set_string(message, "mode", args[2])
    }
    let reply = xpc_connection_send_message_with_reply_sync(connection, message)
    guard xpc_get_type(reply) == XPC_TYPE_DICTIONARY else {
        print(#"{"ok":false,"error":"helper not reachable"}"#)
        return 2
    }
    var json: [String: Any] = ["ok": xpc_dictionary_get_bool(reply, "ok")]
    if let error = xpc_dictionary_get_string(reply, "error") { json["error"] = String(cString: error) }
    if let list = xpc_dictionary_get_value(reply, "fans") {
        var out: [[String: Any]] = []
        for i in 0..<xpc_array_get_count(list) {
            let entry = xpc_array_get_value(list, i)
            out.append([
                "index": xpc_dictionary_get_int64(entry, "index"),
                "forced": xpc_dictionary_get_bool(entry, "forced"),
                "rpm": xpc_dictionary_get_double(entry, "rpm"),
            ])
        }
        json["fans"] = out
    }
    let data = (try? JSONSerialization.data(withJSONObject: json)) ?? Data("{}".utf8)
    print(String(decoding: data, as: UTF8.self))
    return xpc_dictionary_get_bool(reply, "ok") ? 0 : 1
}

// MARK: - Registration

func runService(_ command: String) -> Int32 {
    let service = SMAppService.daemon(plistName: "\(label).plist")
    do {
        switch command {
        case "register": try service.register()
        case "unregister": try service.unregister()
        default: break
        }
    } catch {
        print(#"{"ok":false,"error":"\#(error.localizedDescription.replacingOccurrences(of: "\"", with: "'"))"}"#)
        return 1
    }
    let status: String
    switch service.status {
    case .enabled: status = "enabled"
    case .requiresApproval: status = "requiresApproval"
    case .notRegistered: status = "notRegistered"
    case .notFound: status = "notFound"
    @unknown default: status = "unknown"
    }
    if status == "requiresApproval", command == "register" { SMAppService.openSystemSettingsLoginItems() }
    print(#"{"ok":true,"status":"\#(status)"}"#)
    return 0
}

// MARK: - Self test

func runSelftest() -> Int32 {
    var failures = 0
    func check(_ ok: Bool, _ what: String) {
        if !ok { failures += 1; print("FAIL \(what)") }
    }
    check(MemoryLayout<SMCParam>.size == 80, "SMCParam is 80 bytes (got \(MemoryLayout<SMCParam>.size))")
    check(MemoryLayout<SMCParam>.offset(of: \SMCParam.info) == 28, "info at 28")
    check(MemoryLayout<SMCParam>.offset(of: \SMCParam.result) == 40, "result at 40")
    check(MemoryLayout<SMCParam>.offset(of: \SMCParam.command) == 42, "command at 42")
    check(MemoryLayout<SMCParam>.offset(of: \SMCParam.index) == 44, "index at 44")
    check(MemoryLayout<SMCParam>.offset(of: \SMCParam.bytes) == 48, "bytes at 48")
    check(writable("F0md") && writable("F1Tg") && writable("Ftst"), "fan keys are writable")
    check(!writable("F0Ac") && !writable("PSTR") && !writable("Tp05") && !writable("FNum") && !writable("F0Mx"), "nothing else is writable")
    check(target(.cool, min: 1000, max: 4900) == 2940, "cool is 60 % of max")
    check(target(.cool, min: 3000, max: 4000) == 3000, "cool never below min")
    check(target(.max, min: 1000, max: 4900) == 4900, "max is max")
    check(target(.auto, min: 1000, max: 4900) == nil, "auto has no target")
    check(fourCC("F0md") == 0x4630_6d64, "four-character codes")
    // Read-only: proves the struct layout against the real SMC without writing anything.
    if let smc = SMC() {
        let count = smc.read("FNum"), min = smc.read("F0Mn"), max = smc.read("F0Mx")
        // Informational: a fanless Mac or a CI virtual machine has no fan keys to read.
        print("smc: \(Int(count ?? 0)) fan(s), F0 \(Int(min ?? -1))–\(Int(max ?? -1)) rpm")
    }
    print(failures == 0 ? "selftest ok" : "selftest: \(failures) failed")
    return failures == 0 ? 0 : 1
}

// MARK: - Entry

let args = Array(CommandLine.arguments.dropFirst())
switch args.first {
case "daemon": runDaemon()
case "client": exit(runClient(Array(args.dropFirst())))
case "register", "unregister", "status": exit(runService(args[0]))
case "selftest": exit(runSelftest())
default:
    print("usage: quotadeck-fanhelper daemon | client <set fan mode|renew|restore|status> | register | unregister | status | selftest")
    exit(64)
}
