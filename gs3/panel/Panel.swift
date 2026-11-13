// gs3-panel: the rail's own window for text it cannot fit in a 50-column pane — a commit with its files
// and the diff of one of them, today. mpv already proves that a window of another process can be placed
// without permissions; this one is the same idea for text: the HUD writes what it wants shown to a file,
// says where, and this program draws it. Closing it is the window's own X, or Esc.
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

let usage = "usage: gs3-panel --geometry WxH+X+Y <file>"

var geometry: NSRect?
var path: String?
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
    default:
        path = arg
    }
}

guard let file = path else { fail("no file to show. " + usage) }
guard let text = try? String(contentsOfFile: file, encoding: .utf8) else { fail("cannot read \(file)") }
// No geometry: a centred 70% x 62% of the screen, shifted left so the window sits over the chat rather
// than over the rail. A text window belongs to the screen, not to a pane's cell grid.
let rect = geometry ?? {
    let screen = NSScreen.main?.frame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
    let w = screen.width * 0.70
    let h = screen.height * 0.62
    let x = max(0, screen.origin.x + (screen.width - w) / 2 - screen.width * 0.14)
    let y = max(0, screen.origin.y + (screen.height - h) / 2)
    return NSRect(x: x, y: y, width: w, height: h)
}()

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
    override func draw(_ dirtyRect: NSRect) {
        let rect = bounds.insetBy(dx: 0.5, dy: 0.5)
        let path = NSBezierPath(roundedRect: rect, xRadius: 9, yRadius: 9)
        NSColor(calibratedRed: 13 / 255, green: 22 / 255, blue: 36 / 255, alpha: 0.98).setFill()
        path.fill()
        NSColor(calibratedRed: 51 / 255, green: 65 / 255, blue: 85 / 255, alpha: 1).setStroke()
        path.lineWidth = 1
        path.stroke()
    }
}

final class Delegate: NSObject, NSApplicationDelegate, NSWindowDelegate, NSTextViewDelegate {
    let text: String
    let rect: NSRect
    var window: NSWindow?

    init(text: String, rect: NSRect) {
        self.text = text
        self.rect = rect
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
        window.title = "gs3 · commit"
        window.delegate = self
        window.isReleasedWhenClosed = false
        window.isMovableByWindowBackground = true
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.level = .floating

        let background = NSColor(calibratedRed: 13 / 255, green: 22 / 255, blue: 36 / 255, alpha: 1)
        window.backgroundColor = background

        let scroll = NSScrollView(frame: NSRect(x: 1, y: 1, width: rect.width - 2, height: rect.height - 2))
        scroll.autoresizingMask = [.width, .height]
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
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
        let close = (body.string as NSString).range(of: "✕")
        if close.location != NSNotFound {
            body.addAttributes([.link: URL(string: "gs3://close")!, .foregroundColor: NSColor(calibratedRed: 251/255, green: 113/255, blue: 133/255, alpha: 1)], range: close)
        }
        view.textStorage?.setAttributedString(body)
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = true
        view.textContainer?.widthTracksTextView = false
        view.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)

        scroll.documentView = view
        let card = CardView(frame: NSRect(x: 0, y: 0, width: rect.width, height: rect.height))
        card.autoresizingMask = [.width, .height]
        card.addSubview(scroll)
        window.contentView = card
        window.makeKeyAndOrderFront(nil)
        view.delegate = self
        window.makeFirstResponder(view)
        NSApplication.shared.activate(ignoringOtherApps: true)
        self.window = window
    }

    func windowWillClose(_ notification: Notification) {
        exit(0)
    }

    func textView(_ view: NSTextView, clickedOnLink link: Any, at index: Int) -> Bool {
        exit(0)
    }
}

let app = NSApplication.shared
// Accessory: no Dock icon, and the terminal keeps its own focus when the window goes away.
app.setActivationPolicy(.accessory)
let delegate = Delegate(text: text, rect: rect)
app.delegate = delegate
NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
    if event.keyCode == 53 { exit(0) }  // Esc
    return event
}
app.run()
