// gs3-panel: the rail's own window for what it cannot fit in a 50-column pane — a commit with its files
// and the diff of one of them, or the text or image of the file clicked in TREE. mpv already proves that a
// window of another process can be placed without permissions; this one is the same idea: the HUD writes
// what it wants shown to a file (or, for an image, just names it), says where, and this program draws it.
// Closing it is the window's own X, or Esc.
//
// It is a plain single-file AppKit program on purpose: no bundle, no storyboard, no Dock icon.
import AppKit

/// Screen height of the main display, in points: CoreGraphics reports a window rectangle with y growing
/// downwards from its top, AppKit with y growing upwards from its bottom.
func primaryScreenHeight() -> CGFloat {
    NSScreen.screens.first?.frame.height ?? 1080
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(2)
}

let usage = "usage: gs3-panel [--geometry WxH+X+Y] [--socket <path>] [--image] <file>"

var geometry: NSRect?
var path: String?
var hud: String?
var image = false
var args = CommandLine.arguments.dropFirst()
while let arg = args.popFirst() {
    switch arg {
    case "--help", "-h":
        print(usage)
        exit(0)
    case "--geometry":
        // The same `WxH+X+Y` the rail already produces for mpv: one format, one place to get it wrong.
        guard let value = args.popFirst() else { fail("--geometry needs WxH+X+Y") }
        let numbers = value.split(whereSeparator: { "x+-".contains($0) }).compactMap { Double($0) }
        let signs = value.filter { $0 == "+" || $0 == "-" }
        guard numbers.count == 4, signs.count == 2, numbers[0] > 0, numbers[1] > 0 else { fail("bad geometry: \(value)") }
        let x = signs.first == "-" ? -numbers[2] : numbers[2]
        let y = signs.last == "-" ? -numbers[3] : numbers[3]
        geometry = NSRect(x: x, y: y, width: numbers[0], height: numbers[1])
    case "--socket":
        hud = args.popFirst()
    case "--image":
        // Drawn, not read: an image's bytes never travel through the rail.
        image = true
    default:
        path = arg
    }
}

guard let file = path else { fail("no file to show. " + usage) }
// An image is decoded by AppKit; a text file is read here, and a colour-coded diff arrives as text too.
var text = ""
if image {
    guard NSImage(contentsOfFile: file) != nil else { fail("cannot read \(file)") }
} else {
    guard let read = try? String(contentsOfFile: file, encoding: .utf8) else { fail("cannot read \(file)") }
    text = read
}
// No geometry: a centred 70% x 62% of the screen, shifted left so the window sits over the chat rather
// than over the rail. A text window belongs to the screen, not to a pane's cell grid.
let rect = geometry ?? {
    let screen = NSScreen.main?.frame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
    let w = screen.width * 0.78
    let h = screen.height * 0.62
    let x = max(0, screen.origin.x + (screen.width - w) / 2 - screen.width * 0.14)
    let y = max(0, screen.origin.y + (screen.height - h) / 2)
    return NSRect(x: x, y: y, width: w, height: h)
}()

/// The ✕ in the header closes, and every file row opens that file's diff in the rail. Matching is done
/// with NSRange on purpose: character indices here are easy to get wrong and a wrong one is a crash.
func decorate(_ body: NSMutableAttributedString) {
    let text = body.string
    let ns = text as NSString
    let close = ns.range(of: "✕")
    if close.location != NSNotFound {
        if let url = URL(string: "gs3://close") {
            body.addAttributes([.link: url, .foregroundColor: NSColor(calibratedRed: 251 / 255, green: 113 / 255, blue: 133 / 255, alpha: 1)], range: close)
        }
    }
    let pattern = try? NSRegularExpression(pattern: "^[MAD?U]  (\\S+)  \\[diff\\]", options: [.anchorsMatchLines])
    pattern?.enumerateMatches(in: text, options: [], range: NSRange(location: 0, length: ns.length)) { match, _, _ in
        guard let match = match else { return }
        let range = match.range(at: 1)
        guard range.location != NSNotFound else { return }
        let path = ns.substring(with: range)
        guard let encoded = path.addingPercentEncoding(withAllowedCharacters: .alphanumerics), let url = URL(string: "gs3://file?" + encoded) else { return }
        body.addAttribute(.link, value: url, range: range)
    }
}

/// SGR in, attributes out: the rail writes the colours it already uses (24-bit foreground and reset), so
/// a diff arrives green for what was added, red for what was removed, and nothing is guessed here.
/// Indexing over `[Character]` on purpose: substring indices here are easy to get wrong and a wrong one
/// is a crash, not a warning.
func attributed(_ text: String) -> NSAttributedString {
    let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
    let plain = NSColor(calibratedWhite: 0.93, alpha: 1)
    let out = NSMutableAttributedString()
    var colour = plain
    var line = ""
    func flush() {
        guard !line.isEmpty else { return }
        out.append(NSAttributedString(string: line, attributes: [.font: font, .foregroundColor: colour]))
        line = ""
    }
    let chars = Array(text)
    var i = 0
    while i < chars.count {
        if chars[i] == "\u{1b}", i + 1 < chars.count, chars[i + 1] == "[" {
            var j = i + 2
            var body = ""
            var closed = false
            while j < chars.count {
                if chars[j] == "m" { closed = true; break }
                body.append(chars[j]); j += 1
            }
            if closed {
                flush()
                let parts = body.split(separator: ";").compactMap { Int($0) }
                if parts.count == 5, parts[0] == 38, parts[1] == 2 {
                    colour = NSColor(calibratedRed: CGFloat(parts[2]) / 255, green: CGFloat(parts[3]) / 255, blue: CGFloat(parts[4]) / 255, alpha: 1)
                } else if parts.isEmpty || parts[0] == 0 {
                    colour = plain
                }
                i = j + 1
                continue
            }
        }
        line.append(chars[i]); i += 1
    }
    flush()
    return out
}

/// The rail draws its sections as rounded cards on the terminal's own background; a window that ignored
/// that read as a piece of another application. This is the same card: the same navy, the same one-pixel
/// border, the same corner radius, with the text inset inside it.
final class CardView: NSView {
    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.cornerRadius = 9
        layer?.borderWidth = 1
        layer?.borderColor = NSColor(calibratedRed: 51 / 255, green: 65 / 255, blue: 85 / 255, alpha: 1).cgColor
        layer?.backgroundColor = NSColor(calibratedRed: 13 / 255, green: 22 / 255, blue: 36 / 255, alpha: 1).cgColor
        layer?.masksToBounds = true
    }
    required init?(coder: NSCoder) { fatalError("not a nib") }
}

final class Delegate: NSObject, NSApplicationDelegate, NSWindowDelegate, NSTextViewDelegate {
    var text: String
    let rect: NSRect
    var window: NSWindow?
    var reloader: Timer?
    let path: String?
    let hud: String?
    let image: Bool
    var picture: NSImageView?
    var stamp: Date?

    init(text: String, rect: NSRect, path: String?, hud: String?, image: Bool) {
        self.text = text
        self.rect = rect
        self.path = path
        self.hud = hud
        self.image = image
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let frame = NSRect(
            x: rect.origin.x,
            y: primaryScreenHeight() - rect.origin.y - rect.height,
            width: rect.width,
            height: rect.height
        )
        // No title bar: the header line carries the close mark, like a terminal panel. The whole surface
        // drags, so a borderless window can still be moved.
        let window = NSWindow(
            contentRect: frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.title = image ? "gs3 · imagen" : "gs3 · commit"
        window.delegate = self
        window.isReleasedWhenClosed = false
        window.isMovableByWindowBackground = true
        window.isOpaque = true
        window.backgroundColor = NSColor(calibratedRed: 13 / 255, green: 22 / 255, blue: 36 / 255, alpha: 1)
        window.hasShadow = true
        window.level = .floating

        let background = NSColor(calibratedRed: 13 / 255, green: 22 / 255, blue: 36 / 255, alpha: 1)
        window.backgroundColor = background

        let card = CardView(frame: NSRect(x: 0, y: 0, width: rect.width, height: rect.height))
        card.autoresizingMask = [.width, .height]
        window.contentView = card
        if image { imageContent(card) } else { textContent(card, window) }
        window.makeKeyAndOrderFront(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
        self.window = window
    }

    /// An image as the window's whole content, keeping its proportions whatever size the window has.
    func imageContent(_ card: CardView) {
        let view = NSImageView(frame: NSRect(x: 10, y: 10, width: rect.width - 20, height: rect.height - 20))
        view.autoresizingMask = [.width, .height]
        view.imageScaling = .scaleProportionallyDown
        view.imageAlignment = .alignCenter
        view.image = path.flatMap { NSImage(contentsOfFile: $0) }
        card.addSubview(view)
        picture = view
        // A borderless window has no system close button: the ✕ here and Esc are the way out, like the
        // text window's ✕, which arrives inside the text the rail wrote.
        let close = NSButton(frame: NSRect(x: rect.width - 38, y: rect.height - 38, width: 26, height: 26))
        close.title = "✕"
        close.isBordered = false
        close.contentTintColor = NSColor(calibratedRed: 251 / 255, green: 113 / 255, blue: 133 / 255, alpha: 1)
        close.target = self
        close.action = #selector(dismiss)
        // Pinned to the top-right corner as the window is resized.
        close.autoresizingMask = [.minXMargin, .minYMargin]
        card.addSubview(close)
        // The rail names the same file again when the image changes under it.
        reloader = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: true) { [weak self] _ in
            self?.reloadImage()
        }
    }

    /// What the rail wrote: a diff, or the text of a file clicked in TREE, with the colours it carries.
    func textContent(_ card: CardView, _ window: NSWindow) {
        let background = NSColor(calibratedRed: 13 / 255, green: 22 / 255, blue: 36 / 255, alpha: 1)
        let scroll = NSScrollView(frame: NSRect(x: 1, y: 1, width: rect.width - 2, height: rect.height - 2))
        scroll.autoresizingMask = [.width, .height]
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = false
        scroll.autoresizingMask = [.width, .height]
        scroll.drawsBackground = false
        scroll.backgroundColor = background

        let view = NSTextView(frame: NSRect(x: 0, y: 0, width: rect.width, height: rect.height))
        view.textContainerInset = NSSize(width: 12, height: 10)
        view.isEditable = false
        view.isSelectable = true
        view.drawsBackground = true
        view.backgroundColor = background
        view.textColor = NSColor(calibratedWhite: 0.93, alpha: 1)
        view.insertionPointColor = background
        view.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        let body = NSMutableAttributedString(attributedString: attributed(text))
        // The ✕ in the header is the close button; ⌘W and Esc still work, so nothing is lost with no title bar.
        decorate(body)
        view.textStorage?.setAttributedString(body)
        // The rail rewrites the file when the answer arrives; the window follows it instead of asking to be
        // respawned, so clicking a file keeps the same window, its place and its scroll.
        reloader = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            self?.reload()
        }
        view.isVerticallyResizable = true
        // No wrapping and no width tracking: a long diff line runs off to the right and the horizontal
        // scroller brings it back, so a whole file can be read at full width.
        view.isHorizontallyResizable = true
        view.isVerticallyResizable = true
        view.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        view.textContainer?.widthTracksTextView = false
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        // Only the height follows the window: a width-tracking view would rewrap and kill the scroller.
        view.autoresizingMask = [.height]

        scroll.documentView = view
        card.addSubview(scroll)
        view.delegate = self
        window.makeFirstResponder(view)
    }

    @objc func dismiss() {
        exit(0)
    }

    /// Follow an image that changed on disk: the rail keeps pointing at the same path.
    func reloadImage() {
        guard let file = path else { return }
        let stamp = (try? FileManager.default.attributesOfItem(atPath: file))?[.modificationDate] as? Date
        guard let date = stamp, date != self.stamp else { return }
        self.stamp = date
        picture?.image = NSImage(contentsOfFile: file)
    }

    func windowWillClose(_ notification: Notification) {
        exit(0)
    }

    func textView(_ view: NSTextView, clickedOnLink link: Any, at index: Int) -> Bool {
        guard let url = link as? URL else { exit(0) }
        if url.scheme == "gs3", url.host == "close" { exit(0) }
        // gs3://file?<percent-encoded path> — the rail turns it into the same click its own card gets.
        if url.scheme == "gs3", url.host == "file" {
            var name = url.path
            if name.hasPrefix("/") { name.removeFirst() }
            if let decoded = name.removingPercentEncoding { tell("file \(decoded)\n") }
        }
        return true
    }

    /// One line to the rail: it owns the socket, so nothing here has to know what a selection is.
    func tell(_ line: String) {
        guard let path = hud else { return }
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        if fd < 0 { return }
        defer { close(fd) }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        withUnsafeMutableBytes(of: &addr.sun_path) { dst in
            for (i, b) in bytes.prefix(max(0, dst.count - 1)).enumerated() { dst[i] = b }
        }
        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        let connected = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { connect(fd, $0, size) == 0 }
        }
        guard connected else { return }
        _ = line.withCString { write(fd, $0, strlen($0)) }
    }

    /// Re-read what the rail wrote. Same hash, new answer: the diff of the file that was clicked.
    func reload() {
        guard let file = path, let fresh = try? String(contentsOfFile: file, encoding: .utf8), fresh != text else { return }
        text = fresh
        let body = NSMutableAttributedString(attributedString: attributed(fresh))
        decorate(body)
        guard let scroll = window?.contentView?.subviews.first as? NSScrollView, let view = scroll.documentView as? NSTextView else { return }
        let top = view.enclosingScrollView?.contentView.bounds.origin ?? .zero
        view.textStorage?.setAttributedString(body)
        view.enclosingScrollView?.contentView.scroll(to: top)
    }
}

let app = NSApplication.shared
// Accessory: no Dock icon, and the terminal keeps its own focus when the window goes away.
app.setActivationPolicy(.accessory)
let delegate = Delegate(text: text, rect: rect, path: file, hud: hud, image: image)
app.delegate = delegate
NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
    if event.keyCode == 53 { exit(0) }  // Esc
    return event
}
app.run()
