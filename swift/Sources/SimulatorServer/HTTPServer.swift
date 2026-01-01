import Foundation

// MARK: - HTTP Server

class HTTPServer {
    private let port: UInt16
    private var socket: Int32 = -1
    private let frameQueue = DispatchQueue(label: "com.simulator-server.http-frames", qos: .userInteractive)
    private var frameBuffer = CircularFrameBuffer(capacity: 5)
    private var clientCount: Int = 0
    private var totalBytesServed: UInt64 = 0

    typealias FrameData = (jpegData: Data, timestamp: TimeInterval)

    init(port: UInt16 = 0) {
        self.port = port
        Logger.debug("HTTPServer initialized with port: \(port == 0 ? "auto" : String(port))")
    }

    func start() -> UInt16? {
        Logger.info("Starting HTTP server...")

        var serverAddr = sockaddr_in()
        serverAddr.sin_family = UInt8(AF_INET)
        serverAddr.sin_port = in_port_t(port).bigEndian
        serverAddr.sin_addr.s_addr = inet_addr("127.0.0.1")

        socket = Darwin.socket(AF_INET, SOCK_STREAM, 0)
        guard socket >= 0 else {
            Logger.error("Cannot create socket: errno=\(errno)")
            return nil
        }
        Logger.debug("Socket created: fd=\(socket)")

        var reuseAddr: Int32 = 1
        if setsockopt(socket, SOL_SOCKET, SO_REUSEADDR, &reuseAddr, socklen_t(MemoryLayout<Int32>.size)) < 0 {
            Logger.error("Cannot set SO_REUSEADDR: errno=\(errno)")
            Darwin.close(socket)
            return nil
        }
        Logger.debug("Socket options set (SO_REUSEADDR)")

        let bindResult = withUnsafePointer(to: &serverAddr) { ptr in
            Darwin.bind(socket, UnsafeRawPointer(ptr).assumingMemoryBound(to: sockaddr.self), socklen_t(MemoryLayout<sockaddr_in>.size))
        }

        guard bindResult == 0 else {
            Logger.error("Cannot bind socket: errno=\(errno)")
            Darwin.close(socket)
            return nil
        }
        Logger.debug("Socket bound to 127.0.0.1:\(port)")

        guard Darwin.listen(socket, 128) == 0 else {
            Logger.error("Cannot listen on socket: errno=\(errno)")
            Darwin.close(socket)
            return nil
        }
        Logger.debug("Socket listening (backlog=128)")

        var actualAddr = sockaddr_in()
        var addrLen = socklen_t(MemoryLayout<sockaddr_in>.size)

        if withUnsafeMutablePointer(to: &actualAddr, { ptr in
            Darwin.getsockname(socket, UnsafeMutableRawPointer(ptr).assumingMemoryBound(to: sockaddr.self), &addrLen)
        }) != 0 {
            Logger.error("Cannot get socket name: errno=\(errno)")
            Darwin.close(socket)
            return nil
        }

        let boundPort = UInt16(bigEndian: actualAddr.sin_port)
        Logger.info("HTTP server listening on 127.0.0.1:\(boundPort)")

        // Start accepting connections in background
        DispatchQueue.global().async { [weak self] in
            self?.acceptConnections()
        }

        return boundPort
    }

    func submitFrame(_ jpegData: Data) {
        frameQueue.async { [weak self] in
            self?.frameBuffer.append(jpegData: jpegData, timestamp: CFAbsoluteTimeGetCurrent())
        }
    }

    func stop() {
        Logger.info("Stopping HTTP server...")
        if socket >= 0 {
            Darwin.close(socket)
            socket = -1
        }
        Logger.info("HTTP server stopped")
    }

    // MARK: - Private

    private func acceptConnections() {
        Logger.info("Accepting connections...")

        while true {
            var clientAddr = sockaddr_in()
            var addrLen = socklen_t(MemoryLayout<sockaddr_in>.size)

            let clientSocket = withUnsafeMutablePointer(to: &clientAddr) { ptr in
                Darwin.accept(socket, UnsafeMutableRawPointer(ptr).assumingMemoryBound(to: sockaddr.self), &addrLen)
            }

            guard clientSocket >= 0 else {
                Logger.warn("accept() failed: errno=\(errno)")
                continue
            }

            clientCount += 1
            let clientId = clientCount
            Logger.info("Client \(clientId) connected (fd=\(clientSocket))")

            DispatchQueue.global().async { [weak self] in
                self?.handleClient(clientSocket, clientId: clientId)
            }
        }
    }

    private func handleClient(_ clientSocket: Int32, clientId: Int) {
        defer {
            Darwin.close(clientSocket)
            Logger.info("Client \(clientId) disconnected")
        }

        let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: 4096)
        defer { buffer.deallocate() }

        // Read HTTP request
        let bytesRead = Darwin.read(clientSocket, buffer, 4096)
        var isOptionsRequest = false
        if bytesRead > 0 {
            let request = String(bytes: Data(bytes: buffer, count: bytesRead), encoding: .utf8) ?? ""
            let firstLine = request.components(separatedBy: "\r\n").first ?? ""
            Logger.debug("Client \(clientId) request: \(firstLine)")

            // Check if this is a CORS preflight request
            if firstLine.hasPrefix("OPTIONS ") {
                isOptionsRequest = true
            }
        }

        // CORS headers needed for cross-origin requests
        let corsHeaders = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Max-Age: 86400\r\n"

        // Handle CORS preflight request
        if isOptionsRequest {
            let optionsResponse = "HTTP/1.1 204 No Content\r\n\(corsHeaders)Content-Length: 0\r\n\r\n"
            _ = optionsResponse.withCString { cstr in
                Darwin.write(clientSocket, cstr, strlen(cstr))
            }
            Logger.debug("Client \(clientId): sent OPTIONS response")
            return
        }

        // Send HTTP response header for MJPEG stream
        let responseHeader = "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=--mjpegstream\r\nConnection: close\r\n\(corsHeaders)\r\n"
        _ = responseHeader.withCString { cstr in
            Darwin.write(clientSocket, cstr, strlen(cstr))
        }
        Logger.debug("Client \(clientId): sent HTTP headers")

        // Stream frames to client
        var framesSent: UInt64 = 0
        var bytesSent: UInt64 = 0
        let startTime = CFAbsoluteTimeGetCurrent()

        // First, send any buffered frames and get the current sequence number
        let (bufferedFrames, lastSequence) = frameQueue.sync { frameBuffer.getAllFrames() }
        var clientSequence = lastSequence
        Logger.debug("Client \(clientId): sending \(bufferedFrames.count) buffered frames, starting at sequence \(clientSequence)")

        for frame in bufferedFrames {
            let frameData = writeMJPEGFrame(frame.jpegData)
            let writeResult = frameData.withUnsafeBytes { ptr in
                Darwin.write(clientSocket, ptr.baseAddress, frameData.count)
            }

            if writeResult < 0 {
                Logger.warn("Client \(clientId): write failed on buffered frame")
                return
            }

            framesSent += 1
            bytesSent += UInt64(frameData.count)
        }

        Logger.debug("Client \(clientId): buffered frames sent, now streaming live from sequence \(clientSequence)")

        // Keep streaming new frames
        while true {
            let (newFrames, newSequence) = frameQueue.sync { frameBuffer.getFramesSince(sequence: clientSequence) }

            guard !newFrames.isEmpty else {
                usleep(1000) // 1ms sleep to avoid busy waiting
                continue
            }

            // Update client's sequence number
            clientSequence = newSequence

            for frame in newFrames {
                let frameData = writeMJPEGFrame(frame.jpegData)
                let writeResult = frameData.withUnsafeBytes { ptr in
                    Darwin.write(clientSocket, ptr.baseAddress, frameData.count)
                }

                if writeResult < 0 {
                    let elapsed = CFAbsoluteTimeGetCurrent() - startTime
                    Logger.info("Client \(clientId): disconnected after \(framesSent) frames, \(bytesSent) bytes, \(String(format: "%.1f", elapsed))s")
                    totalBytesServed += bytesSent
                    return
                }

                framesSent += 1
                bytesSent += UInt64(frameData.count)

                // Log progress every 100 frames
                if framesSent % 100 == 0 {
                    Logger.debug("Client \(clientId): sent \(framesSent) frames, \(bytesSent) bytes")
                }
            }
        }
    }

    private func writeMJPEGFrame(_ jpegData: Data) -> Data {
        let boundary = "--mjpegstream"
        let contentLength = jpegData.count

        var result = Data()

        // Write boundary and headers
        let headerString = "\(boundary)\r\nContent-Type: image/jpeg\r\nContent-Length: \(contentLength)\r\n\r\n"
        result.append(headerString.data(using: .utf8)!)

        // Write JPEG data
        result.append(jpegData)

        // Write trailing CRLF
        result.append("\r\n".data(using: .utf8)!)

        return result
    }
}

// MARK: - Circular Frame Buffer

class CircularFrameBuffer {
    private let capacity: Int
    private var frames: [(jpegData: Data, timestamp: TimeInterval, sequenceNumber: UInt64)] = []
    private var nextSequenceNumber: UInt64 = 0
    private let lock = NSLock()

    init(capacity: Int) {
        self.capacity = capacity
        Logger.debug("CircularFrameBuffer initialized with capacity: \(capacity)")
    }

    func append(jpegData: Data, timestamp: TimeInterval) {
        lock.lock()
        defer { lock.unlock() }

        frames.append((jpegData, timestamp, nextSequenceNumber))
        nextSequenceNumber += 1

        if frames.count > capacity {
            frames.removeFirst()
        }
    }

    /// Get all current frames and return the sequence number of the last frame
    func getAllFrames() -> (frames: [(jpegData: Data, timestamp: TimeInterval)], lastSequence: UInt64) {
        lock.lock()
        defer { lock.unlock() }

        let result = frames.map { ($0.jpegData, $0.timestamp) }
        return (result, nextSequenceNumber)
    }

    /// Get frames newer than the given sequence number
    func getFramesSince(sequence: UInt64) -> (frames: [(jpegData: Data, timestamp: TimeInterval)], lastSequence: UInt64) {
        lock.lock()
        defer { lock.unlock() }

        // Find frames with sequence number >= given sequence
        let newFrames = frames.filter { $0.sequenceNumber >= sequence }
            .map { ($0.jpegData, $0.timestamp) }

        return (newFrames, nextSequenceNumber)
    }
}
