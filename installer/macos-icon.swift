import AppKit

// Draw the website's existing vector mark at each macOS icon resolution.
let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = size * scale
        let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
            isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        )!
        let context = NSGraphicsContext(bitmapImageRep: bitmap)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        context.cgContext.scaleBy(x: CGFloat(pixels) / 48, y: CGFloat(pixels) / 48)
        NSColor(srgbRed: 16 / 255, green: 19 / 255, blue: 19 / 255, alpha: 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: 2, y: 2, width: 44, height: 44), xRadius: 10, yRadius: 10).fill()
        let mark = NSBezierPath()
        mark.lineWidth = 3
        mark.lineJoinStyle = .bevel
        for points in [[(13, 10), (13, 38)], [(35, 10), (35, 38)],
                       [(13, 17), (21, 17), (27, 31), (35, 31)],
                       [(13, 31), (21, 31), (27, 17), (35, 17)]] {
            mark.move(to: NSPoint(x: points[0].0, y: points[0].1))
            for point in points.dropFirst() {
                mark.line(to: NSPoint(x: point.0, y: point.1))
            }
        }
        NSColor(srgbRed: 192 / 255, green: 239 / 255, blue: 131 / 255, alpha: 1).setStroke()
        mark.stroke()
        NSGraphicsContext.restoreGraphicsState()
        let suffix = scale == 2 ? "@2x" : ""
        try bitmap.representation(using: .png, properties: [:])!
            .write(to: directory.appendingPathComponent("icon_\(size)x\(size)\(suffix).png"))
    }
}
