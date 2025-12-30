import Foundation
import CoreGraphics
import ImageIO
import IOSurface
import CoreVideo
import VideoToolbox
import CoreMedia

// MARK: - CoreSimulator Private APIs

@objc protocol SimDeviceIOProtocol {
    var ioPorts: [Any] { get }
}

@objc protocol SimDeviceIOPortInterface {
    var descriptor: Any { get }
}

@objc protocol SimDisplayRenderable {
    var ioSurface: IOSurface? { get }
    @objc optional var framebufferSurface: IOSurface? { get }
    func registerCallbackWithUUID(_ uuid: UUID, ioSurfaceChangeCallback: @escaping (IOSurface?) -> Void)
    func registerCallbackWithUUID(_ uuid: UUID, ioSurfacesChangeCallback: @escaping (IOSurface?) -> Void)
    func registerCallbackWithUUID(_ uuid: UUID, damageRectanglesCallback: @escaping ([NSValue]) -> Void)
    func unregisterIOSurfaceChangeCallbackWithUUID(_ uuid: UUID)
    func unregisterIOSurfacesChangeCallbackWithUUID(_ uuid: UUID)
    func unregisterDamageRectanglesCallbackWithUUID(_ uuid: UUID)
}

@objc protocol SimDisplayDescriptorState {
    var displayClass: UInt16 { get }
}

@objc protocol SimDevice {
    var io: Any? { get }
    var udid: UUID { get }
    var name: String { get }
}

@objc protocol SimDeviceSet {
    var devices: [Any] { get }
}

// MARK: - SimServiceContext for accessing devices

func getSimDevice(udid: String) -> Any? {
    guard let coreSimBundle = Bundle(path: "/Library/Developer/PrivateFrameworks/CoreSimulator.framework") else {
        fputs("Error: Cannot load CoreSimulator.framework\n", stderr)
        return nil
    }

    guard coreSimBundle.load() else {
        fputs("Error: Cannot load CoreSimulator bundle\n", stderr)
        return nil
    }

    guard let contextClass = NSClassFromString("SimServiceContext") as? NSObject.Type else {
        fputs("Error: Cannot find SimServiceContext class\n", stderr)
        return nil
    }

    let sharedSelector = NSSelectorFromString("sharedServiceContextForDeveloperDir:error:")
    guard contextClass.responds(to: sharedSelector) else {
        fputs("Error: SimServiceContext doesn't respond to sharedServiceContextForDeveloperDir:error:\n", stderr)
        return nil
    }

    let developerDir = "/Applications/Xcode.app/Contents/Developer" as NSString

    typealias SharedContextMethod = @convention(c) (AnyClass, Selector, NSString, AutoreleasingUnsafeMutablePointer<NSError?>?) -> AnyObject?
    let sharedContextImp = unsafeBitCast(contextClass.method(for: sharedSelector), to: SharedContextMethod.self)

    var error: NSError?
    let context = sharedContextImp(contextClass, sharedSelector, developerDir, &error) as? NSObject

    if let error = error {
        fputs("Error getting service context: \(error)\n", stderr)
        return nil
    }

    guard let context = context else {
        fputs("Error: Cannot get SimServiceContext\n", stderr)
        return nil
    }

    let deviceSetSelector = NSSelectorFromString("defaultDeviceSetWithError:")
    guard context.responds(to: deviceSetSelector) else {
        fputs("Error: Context doesn't respond to defaultDeviceSetWithError:\n", stderr)
        return nil
    }

    typealias DeviceSetMethod = @convention(c) (AnyObject, Selector, AutoreleasingUnsafeMutablePointer<NSError?>?) -> AnyObject?
    let deviceSetImp = unsafeBitCast(type(of: context).instanceMethod(for: deviceSetSelector)!, to: DeviceSetMethod.self)

    let deviceSet = deviceSetImp(context, deviceSetSelector, &error) as? NSObject

    if let error = error {
        fputs("Error getting device set: \(error)\n", stderr)
        return nil
    }

    guard let deviceSet = deviceSet else {
        fputs("Error: Cannot get device set\n", stderr)
        return nil
    }

    let devicesSelector = NSSelectorFromString("devices")
    guard deviceSet.responds(to: devicesSelector) else {
        fputs("Error: DeviceSet doesn't respond to devices\n", stderr)
        return nil
    }

    guard let devices = deviceSet.perform(devicesSelector)?.takeUnretainedValue() as? [AnyObject] else {
        fputs("Error: Cannot get devices array\n", stderr)
        return nil
    }

    let targetUUID = UUID(uuidString: udid)
    for device in devices {
        let udidSelector = NSSelectorFromString("UDID")
        if device.responds(to: udidSelector),
           let deviceUUID = device.perform(udidSelector)?.takeUnretainedValue() as? UUID,
           deviceUUID == targetUUID {
            return device
        }
    }

    fputs("Error: Device with UDID \(udid) not found\n", stderr)
    return nil
}

// MARK: - Hardware-accelerated JPEG Encoder using VideoToolbox

class JPEGEncoder {
    private var compressionSession: VTCompressionSession?
    private var encodedData: Data?
    private let quality: Float
    private let semaphore = DispatchSemaphore(value: 0)

    init(width: Int, height: Int, quality: Float) {
        self.quality = quality

        var session: VTCompressionSession?
        let status = VTCompressionSessionCreate(
            allocator: kCFAllocatorDefault,
            width: Int32(width),
            height: Int32(height),
            codecType: kCMVideoCodecType_JPEG,
            encoderSpecification: [
                kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder: true
            ] as CFDictionary,
            imageBufferAttributes: nil,
            compressedDataAllocator: nil,
            outputCallback: nil,
            refcon: nil,
            compressionSessionOut: &session
        )

        if status == noErr, let session = session {
            self.compressionSession = session

            VTSessionSetProperty(session, key: kVTCompressionPropertyKey_Quality, value: quality as CFNumber)
            VTCompressionSessionPrepareToEncodeFrames(session)
        }
    }

    deinit {
        if let session = compressionSession {
            VTCompressionSessionInvalidate(session)
        }
    }

    func encode(_ pixelBuffer: CVPixelBuffer) -> Data? {
        guard let session = compressionSession else {
            return encodeWithCoreGraphics(pixelBuffer)
        }

        var resultData: Data?
        let presentationTime = CMTime(value: 0, timescale: 1)

        let status = VTCompressionSessionEncodeFrame(
            session,
            imageBuffer: pixelBuffer,
            presentationTimeStamp: presentationTime,
            duration: .invalid,
            frameProperties: nil,
            infoFlagsOut: nil
        ) { [weak self] status, _, sampleBuffer in
            guard status == noErr, let sampleBuffer = sampleBuffer else { return }

            if let dataBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) {
                var length = 0
                var dataPointer: UnsafeMutablePointer<Int8>?
                CMBlockBufferGetDataPointer(dataBuffer, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &length, dataPointerOut: &dataPointer)

                if let dataPointer = dataPointer {
                    resultData = Data(bytes: dataPointer, count: length)
                }
            }
            self?.semaphore.signal()
        }

        if status == noErr {
            _ = semaphore.wait(timeout: .now() + 0.1)
            return resultData
        }

        return encodeWithCoreGraphics(pixelBuffer)
    }

    private func encodeWithCoreGraphics(_ pixelBuffer: CVPixelBuffer) -> Data? {
        CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly) }

        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let bytesPerRow = CVPixelBufferGetBytesPerRow(pixelBuffer)
        let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer)

        let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
        guard let context = CGContext(
            data: baseAddress,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: bytesPerRow,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        ) else {
            return nil
        }

        guard let cgImage = context.makeImage() else {
            return nil
        }

        let mutableData = CFDataCreateMutable(nil, 0)!
        guard let destination = CGImageDestinationCreateWithData(mutableData, "public.jpeg" as CFString, 1, nil) else {
            return nil
        }

        let options: [CFString: Any] = [
            kCGImageDestinationLossyCompressionQuality: quality
        ]
        CGImageDestinationAddImage(destination, cgImage, options as CFDictionary)

        guard CGImageDestinationFinalize(destination) else {
            return nil
        }

        return mutableData as Data
    }
}

// MARK: - IOSurface to CVPixelBuffer

func createPixelBuffer(from surface: IOSurface) -> CVPixelBuffer? {
    var pixelBuffer: Unmanaged<CVPixelBuffer>?
    let status = CVPixelBufferCreateWithIOSurface(
        kCFAllocatorDefault,
        surface,
        nil,
        &pixelBuffer
    )
    return status == kCVReturnSuccess ? pixelBuffer?.takeRetainedValue() : nil
}

// MARK: - MJPEG Stream

func writeMJPEGFrame(_ data: Data, boundary: String = "--mjpegstream") {
    let header = "\(boundary)\r\nContent-Type: image/jpeg\r\nContent-Length: \(data.count)\r\n\r\n"
    FileHandle.standardOutput.write(header.data(using: .utf8)!)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write("\r\n".data(using: .utf8)!)
}

// MARK: - Main

func printUsage() {
    fputs("""
    Usage: plasma-stream --udid <simulator-udid> [options]

    Options:
      --udid <udid>     Simulator UDID (required)
      --fps <fps>       Target frames per second (default: 60)
      --quality <q>     JPEG quality 0.0-1.0 (default: 0.7)
      --help            Show this help

    Output: MJPEG stream to stdout with HTTP multipart headers

    """, stderr)
}

func main() {
    var udid: String?
    var fps: Int = 60
    var quality: Float = 0.7

    var args = CommandLine.arguments.dropFirst()
    while let arg = args.popFirst() {
        switch arg {
        case "--udid":
            udid = args.popFirst()
        case "--fps":
            if let val = args.popFirst(), let intVal = Int(val) {
                fps = min(120, max(1, intVal))
            }
        case "--quality":
            if let val = args.popFirst(), let fltVal = Float(val) {
                quality = min(1.0, max(0.1, fltVal))
            }
        case "--help", "-h":
            printUsage()
            exit(0)
        default:
            break
        }
    }

    guard let udid = udid else {
        fputs("Error: --udid is required\n", stderr)
        printUsage()
        exit(1)
    }

    fputs("Starting plasma-stream for \(udid)\n", stderr)
    fputs("FPS: \(fps), Quality: \(quality)\n", stderr)

    guard let device = getSimDevice(udid: udid) as? NSObject else {
        fputs("Error: Cannot get simulator device\n", stderr)
        exit(1)
    }

    let ioSelector = NSSelectorFromString("io")
    guard device.responds(to: ioSelector),
          let ioClient = device.perform(ioSelector)?.takeUnretainedValue() as? NSObject else {
        fputs("Error: Cannot get IO client from device\n", stderr)
        exit(1)
    }

    let ioPortsSelector = NSSelectorFromString("ioPorts")
    guard ioClient.responds(to: ioPortsSelector),
          let ioPorts = ioClient.perform(ioPortsSelector)?.takeUnretainedValue() as? [AnyObject] else {
        fputs("Error: Cannot get IO ports\n", stderr)
        exit(1)
    }

    fputs("Found \(ioPorts.count) IO ports\n", stderr)

    var mainDisplaySurface: IOSurface?
    var displayDescriptor: NSObject?

    for port in ioPorts {
        guard let portObj = port as? NSObject else { continue }

        let descriptorSelector = NSSelectorFromString("descriptor")
        guard portObj.responds(to: descriptorSelector),
              let descriptor = portObj.perform(descriptorSelector)?.takeUnretainedValue() as? NSObject else {
            continue
        }

        let ioSurfaceSelector = NSSelectorFromString("ioSurface")
        let framebufferSurfaceSelector = NSSelectorFromString("framebufferSurface")

        var surface: IOSurface?

        if descriptor.responds(to: framebufferSurfaceSelector),
           let fb = descriptor.perform(framebufferSurfaceSelector)?.takeUnretainedValue() as? IOSurface {
            surface = fb
        } else if descriptor.responds(to: ioSurfaceSelector),
                  let ios = descriptor.perform(ioSurfaceSelector)?.takeUnretainedValue() as? IOSurface {
            surface = ios
        }

        if let surface = surface {
            let width = IOSurfaceGetWidth(surface)
            let height = IOSurfaceGetHeight(surface)
            fputs("Found surface: \(width)x\(height)\n", stderr)

            let stateSelector = NSSelectorFromString("state")
            if descriptor.responds(to: stateSelector),
               let state = descriptor.perform(stateSelector)?.takeUnretainedValue() as? NSObject {
                let displayClassSelector = NSSelectorFromString("displayClass")
                if state.responds(to: displayClassSelector) {
                    typealias DisplayClassMethod = @convention(c) (AnyObject, Selector) -> UInt16
                    let displayClassImp = unsafeBitCast(type(of: state).instanceMethod(for: displayClassSelector)!, to: DisplayClassMethod.self)
                    let displayClass = displayClassImp(state, displayClassSelector)
                    fputs("  Display class: \(displayClass)\n", stderr)
                    if displayClass == 0 {
                        mainDisplaySurface = surface
                        displayDescriptor = descriptor
                        fputs("  -> Selected as main display\n", stderr)
                        break
                    }
                }
            }

            if mainDisplaySurface == nil || (width * height > IOSurfaceGetWidth(mainDisplaySurface!) * IOSurfaceGetHeight(mainDisplaySurface!)) {
                mainDisplaySurface = surface
                displayDescriptor = descriptor
            }
        }
    }

    guard let surface = mainDisplaySurface, let _ = displayDescriptor else {
        fputs("Error: Cannot find display surface\n", stderr)
        exit(1)
    }

    let width = IOSurfaceGetWidth(surface)
    let height = IOSurfaceGetHeight(surface)

    // Create hardware-accelerated JPEG encoder
    let encoder = JPEGEncoder(width: width, height: height, quality: quality)

    // Write HTTP header
    let httpHeader = "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=--mjpegstream\r\n\r\n"
    FileHandle.standardOutput.write(httpHeader.data(using: .utf8)!)

    fputs("Streaming started...\n", stderr)

    // Stream loop with precise timing
    let frameInterval = 1.0 / Double(fps)
    var frameCount: UInt64 = 0
    let startTime = CFAbsoluteTimeGetCurrent()
    var lastFrameTime = startTime

    while true {
        // Get current surface (it may change)
        let currentSurface = surface

        if let pixelBuffer = createPixelBuffer(from: currentSurface),
           let jpegData = encoder.encode(pixelBuffer) {
            writeMJPEGFrame(jpegData)
            frameCount += 1

            if frameCount % UInt64(fps) == 0 {
                let elapsed = CFAbsoluteTimeGetCurrent() - startTime
                let actualFps = Double(frameCount) / elapsed
                fputs("Frames: \(frameCount), FPS: \(String(format: "%.1f", actualFps))\n", stderr)
            }
        }

        // Precise frame timing using spin-wait for the last microseconds
        let targetTime = lastFrameTime + frameInterval
        while CFAbsoluteTimeGetCurrent() < targetTime - 0.001 {
            Thread.sleep(forTimeInterval: 0.0005)
        }
        while CFAbsoluteTimeGetCurrent() < targetTime {
            // Spin-wait for precise timing
        }
        lastFrameTime = targetTime
    }
}

main()
