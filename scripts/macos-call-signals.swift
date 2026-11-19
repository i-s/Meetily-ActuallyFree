// Read-only metadata probe for issue #7. No tap, audio stream, AX prompt or RPC.
// Compile separately; run only during the owner's batched physical acceptance.
import CoreAudio
import Foundation

func property<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector,
                 initial: T) -> (T?, OSStatus) {
    var address = AudioObjectPropertyAddress(mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    guard AudioObjectHasProperty(object, &address) else { return (nil, -1) }
    var value = initial
    var size = UInt32(MemoryLayout<T>.size)
    let status = AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value)
    return (status == noErr ? value : nil, status)
}

// Size can change when a helper exits/starts. Report failure, never an empty
// successful list, when a property read races process enumeration.
@available(macOS 14.2, *)
func processes() -> ([AudioObjectID]?, OSStatus) {
    var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyProcessObjectList,
        mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    let system = AudioObjectID(kAudioObjectSystemObject)
    guard AudioObjectHasProperty(system, &address) else { return (nil, -1) }
    var size: UInt32 = 0
    var status = AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size)
    guard status == noErr else { return (nil, status) }
    var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    if ids.isEmpty { return ([], noErr) }
    status = ids.withUnsafeMutableBytes {
        AudioObjectGetPropertyData(system, &address, 0, nil, &size, $0.baseAddress!)
    }
    guard status == noErr, Int(size) <= ids.count * MemoryLayout<AudioObjectID>.size else {
        return (nil, status == noErr ? -2 : status)
    }
    return (Array(ids.prefix(Int(size) / MemoryLayout<AudioObjectID>.size)), noErr)
}

if #available(macOS 14.2, *) {
    let (ids, listStatus) = processes()
    var rows: [[String: Any]] = []
    for id in ids ?? [] {
        let (pid, pidStatus): (Int32?, OSStatus) = property(id, kAudioProcessPropertyPID, initial: Int32(0))
        let (bundle, bundleStatus): (CFString?, OSStatus) = property(id, kAudioProcessPropertyBundleID, initial: "" as CFString)
        let (input, inputStatus): (UInt32?, OSStatus) = property(id, kAudioProcessPropertyIsRunningInput, initial: UInt32(0))
        let (output, outputStatus): (UInt32?, OSStatus) = property(id, kAudioProcessPropertyIsRunningOutput, initial: UInt32(0))
        rows.append(["object_id": id, "pid": pid.map { $0 as Any } ?? NSNull(),
            "bundle_id": bundle.map { $0 as String } ?? "",
            "input": input.map { ($0 != 0) as Any } ?? NSNull(),
            "output": output.map { ($0 != 0) as Any } ?? NSNull(),
            "status": ["pid": pidStatus, "bundle_id": bundleStatus,
                       "input": inputStatus, "output": outputStatus]])
    }
    let (inputDevice, inputStatus): (UInt32?, OSStatus) = property(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultInputDevice, initial: UInt32(0))
    let (outputDevice, outputStatus): (UInt32?, OSStatus) = property(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice, initial: UInt32(0))
    let result: [String: Any] = ["probe_version": 1, "list_status": listStatus,
        "processes": rows, "default_input": inputDevice.map { $0 as Any } ?? NSNull(),
        "default_output": outputDevice.map { $0 as Any } ?? NSNull(),
        "device_status": ["input": inputStatus, "output": outputStatus]]
    let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
    print(String(decoding: data, as: UTF8.self))
} else {
    fputs("Core Audio process metadata requires macOS 14.2+ and a compatible SDK\n", stderr)
    exit(2)
}
