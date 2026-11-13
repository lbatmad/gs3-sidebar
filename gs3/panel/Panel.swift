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

let usage = "usage: gs3-panel [--geometry WxH+X+Y] [--socket <path>] [--image] [--markdown] [--cut] [--snapshot <png>] [--width <px>] <file>"

var geometry: NSRect?
var path: String?
var hud: String?
var image = false
var markdown = false
var cut = false
var snapshot: String?
var width: CGFloat = 900
var title: String?
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
    case "--markdown":
        // Rendered, not dumped: headings, lists and tables are the whole point of the file.
        markdown = true
    case "--cut":
        // The rail stopped the file at its ceiling; the window says so instead of pretending it ended.
        cut = true
    case "--snapshot":
        guard let value = args.popFirst() else { fail("--snapshot needs a png path") }
        snapshot = value
    case "--width":
        guard let value = args.popFirst(), let pixels = Double(value), pixels > 0 else { fail("--width needs a positive number") }
        width = CGFloat(pixels)
    case "--title":
        // The rail knows the file's real name; the path it hands over is its own temporary copy.
        title = args.popFirst()
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

// The card's colours in one place: the window, the snapshot and the markdown renderer draw the same
// surface, and a screenshot that disagreed with the window would be worthless as a review.
let panelBackground = NSColor(calibratedRed: 13 / 255, green: 22 / 255, blue: 36 / 255, alpha: 1)
let panelBorder = NSColor(calibratedRed: 51 / 255, green: 65 / 255, blue: 85 / 255, alpha: 1)
let panelText = NSColor(calibratedRed: 0xE2 / 255, green: 0xE8 / 255, blue: 0xF0 / 255, alpha: 1)
let toneGold = NSColor(calibratedRed: 0xFF / 255, green: 0xD2 / 255, blue: 0x3F / 255, alpha: 1)
let toneCyan = NSColor(calibratedRed: 0x06 / 255, green: 0xB6 / 255, blue: 0xD4 / 255, alpha: 1)
let toneCoral = NSColor(calibratedRed: 0xFB / 255, green: 0x71 / 255, blue: 0x85 / 255, alpha: 1)
let toneBlue = NSColor(calibratedRed: 0x38 / 255, green: 0xBD / 255, blue: 0xF8 / 255, alpha: 1)
let toneMuted = NSColor(calibratedRed: 0x94 / 255, green: 0xA3 / 255, blue: 0xB8 / 255, alpha: 1)
let toneDim = NSColor(calibratedRed: 0x84 / 255, green: 0x94 / 255, blue: 0xA8 / 255, alpha: 1)
let codeBackground = NSColor(calibratedRed: 0x1B / 255, green: 0x2A / 255, blue: 0x3F / 255, alpha: 1)
let codeText = NSColor(calibratedRed: 0xC9 / 255, green: 0xD6 / 255, blue: 0xE5 / 255, alpha: 1)

/// Draw what the window would draw into a PNG and leave: the review of this program is a picture, and the
/// README's screenshots come from here too. No window, no screen, no permission.
func snapshotPNG(_ body: NSAttributedString, width pixels: CGFloat, to file: String) -> Never {
    let inset: CGFloat = 16
    let view = NSTextView(frame: NSRect(x: 0, y: 0, width: pixels, height: 160))
    view.isEditable = false
    view.drawsBackground = true
    view.backgroundColor = panelBackground
    view.textContainerInset = NSSize(width: inset, height: inset)
    view.textContainer?.lineFragmentPadding = 0
    // Explicit width, not a tracking one: the view is never in a scroll view here, and a tracking container
    // keeps the width it was born with, which draws every paragraph past the right edge of the PNG.
    view.textContainer?.widthTracksTextView = false
    view.textContainer?.containerSize = NSSize(width: pixels - inset * 2, height: CGFloat.greatestFiniteMagnitude)
    view.textStorage?.setAttributedString(body)
    guard let container = view.textContainer, let layout = view.layoutManager else { fail("snapshot: no text layout") }
    layout.ensureLayout(for: container)
    let used = layout.usedRect(for: container)
    let height = max(160, ceil(used.height + inset * 2))
    view.frame = NSRect(x: 0, y: 0, width: pixels, height: height)
    layout.ensureLayout(for: container)
    guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { fail("snapshot: no bitmap") }
    view.cacheDisplay(in: view.bounds, to: rep)
    guard let png = rep.representation(using: .png, properties: [:]) else { fail("snapshot: no png") }
    do { try png.write(to: URL(fileURLWithPath: file)) } catch { fail("cannot write \(file): \(error)") }
    exit(0)
}

if let out = snapshot {
    if image {
        guard let source = NSImage(contentsOfFile: file) else { fail("cannot read \(file)") }
        let scale = min(1, width / max(source.size.width, 1))
        let size = NSSize(width: max(1, source.size.width * scale), height: max(1, source.size.height * scale))
        guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width), pixelsHigh: Int(size.height),
                                         bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                         colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
              let context = NSGraphicsContext(bitmapImageRep: rep) else { fail("snapshot: no bitmap") }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        source.draw(in: NSRect(origin: .zero, size: size))
        NSGraphicsContext.restoreGraphicsState()
        guard let png = rep.representation(using: .png, properties: [:]) else { fail("snapshot: no png") }
        do { try png.write(to: URL(fileURLWithPath: out)) } catch { fail("cannot write \(out): \(error)") }
        exit(0)
    }
    snapshotPNG(markdown ? markdownBody(text, title: title ?? (file as NSString).lastPathComponent, cut: cut, width: width - 32) : attributed(text), width: width, to: out)
}

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
    var bold = false
    var italic = false
    var line = ""
    func flush() {
        guard !line.isEmpty else { return }
        // Bold and italic arrive as their own SGR codes: a keyword is bold in the rail, and a comment slants.
        var drawn = font
        if bold { drawn = NSFontManager.shared.convert(drawn, toHaveTrait: .boldFontMask) }
        if italic { drawn = NSFontManager.shared.convert(drawn, toHaveTrait: .italicFontMask) }
        out.append(NSAttributedString(string: line, attributes: [.font: drawn, .foregroundColor: colour]))
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
                    colour = plain; bold = false; italic = false
                } else if parts == [1] {
                    bold = true
                } else if parts == [3] {
                    italic = true
                } else if parts == [22] {
                    bold = false
                } else if parts == [23] {
                    italic = false
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

/// Where one piece of a markdown file begins and ends: the parser flattens GFM tables, so the source is cut
/// and each piece is rendered by what it is.
enum MarkdownSegment { case text(String); case table([[String]]) }

/// A table row starts and ends with `|`; anything else belongs to the text around it. Emphasis noise is
/// dropped because a table cell has no room for it and the columns matter more than the markers.
func tableCells(_ line: String) -> [String] {
    // `\|` is a pipe inside a cell, not a separator: it is hidden while splitting and put back after.
    let hidden = line.replacingOccurrences(of: "\\|", with: "\u{0}")
    var cells = hidden.split(separator: "|", omittingEmptySubsequences: false).map { cell -> String in
        cell.trimmingCharacters(in: .whitespaces)
            .replacingOccurrences(of: "\u{0}", with: "|")
            .replacingOccurrences(of: "**", with: "")
            .replacingOccurrences(of: "`", with: "")
            .replacingOccurrences(of: "*", with: "")
    }
    if cells.first == "" { cells.removeFirst() }
    if cells.last == "" { cells.removeLast() }
    return cells
}

/// `|---|---|` and friends: the row that says "this is a table", never a row on screen.
func isTableSeparator(_ line: String) -> Bool {
    let trimmed = line.trimmingCharacters(in: .whitespaces)
    guard trimmed.hasPrefix("|"), trimmed.contains("-") else { return false }
    return trimmed.allSatisfy { $0 == "|" || $0 == "-" || $0 == ":" || $0 == " " }
}

func markdownSegments(_ source: String) -> [MarkdownSegment] {
    let lines = source.components(separatedBy: "\n")
    var segments: [MarkdownSegment] = []
    var buffer: [String] = []
    var index = 0
    func flush() {
        guard !buffer.isEmpty else { return }
        segments.append(.text(buffer.joined(separator: "\n")))
        buffer = []
    }
    while index < lines.count {
        if lines[index].hasPrefix("|"), index + 1 < lines.count, isTableSeparator(lines[index + 1]) {
            flush()
            var rows: [[String]] = []
            while index < lines.count, lines[index].hasPrefix("|") {
                if !isTableSeparator(lines[index]) { rows.append(tableCells(lines[index])) }
                index += 1
            }
            segments.append(.table(rows))
        } else {
            buffer.append(lines[index])
            index += 1
        }
    }
    flush()
    return segments
}

/// A cell that fits the column, broken on spaces and, when a single word is wider than the column, cut: a
/// table whose columns jump is not a table, and a URL in a narrow column has no other way to stay in it.
func wrappedCell(_ text: String, width: Int) -> [String] {
    guard width > 3 else { return [text] }
    var lines: [String] = []
    var line = ""
    func flush() { if !line.isEmpty { lines.append(line); line = "" } }
    for word in text.split(separator: " ") {
        var piece = String(word)
        while piece.count > width {
            flush()
            lines.append(String(piece.prefix(width)))
            piece = String(piece.dropFirst(width))
        }
        let candidate = line.isEmpty ? piece : line + " " + piece
        if candidate.count <= width { line = candidate } else { flush(); line = piece }
    }
    flush()
    return lines.isEmpty ? [""] : lines
}

/// A table as monospaced columns: the point is that the reader sees columns, not pipes. When the columns do
/// not fit — and in these docs they are prose, not numbers — the same rows are drawn as records (a labelled
/// line per row), because a grid squeezed to 20 characters per column is unreadable.
func renderedTable(_ rows: [[String]], width: CGFloat) -> NSAttributedString {
    guard let first = rows.first, !first.isEmpty else { return NSAttributedString() }
    let columns = rows.map(\.count).max() ?? first.count
    // 12 pt monospaced is about 7.2 px per character.
    let available = max(30, Int(width / 7.2) - 3 * max(0, columns - 1))
    var natural = [Int](repeating: 0, count: columns)
    for row in rows { for (i, cell) in row.enumerated() where i < columns { natural[i] = max(natural[i], cell.count) } }
    let mono = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
    let bold = NSFont.monospacedSystemFont(ofSize: 12, weight: .bold)
    let tight = NSMutableParagraphStyle()
    tight.lineSpacing = 1
    tight.paragraphSpacing = 0
    let out = NSMutableAttributedString()
    if natural.reduce(0, +) + 3 * max(0, columns - 1) > available {
        // Records: the first cell names the row and every other cell is a paragraph under it.
        for row in rows.dropFirst() {
            guard let label = row.first, !label.isEmpty else { continue }
            let style = NSMutableParagraphStyle()
            style.lineSpacing = 2
            style.paragraphSpacing = 2
            style.paragraphSpacingBefore = 6
            out.append(NSAttributedString(string: "▸ ", attributes: [.font: bold, .foregroundColor: toneGold, .paragraphStyle: style]))
            out.append(NSAttributedString(string: label, attributes: [.font: bold, .foregroundColor: toneGold, .paragraphStyle: style]))
            out.append(NSAttributedString(string: "\n", attributes: [.font: bold, .paragraphStyle: style]))
            let inset = NSMutableParagraphStyle()
            inset.lineSpacing = 2
            inset.paragraphSpacing = 2
            inset.firstLineHeadIndent = 14
            inset.headIndent = 14
            for value in row.dropFirst() where !value.isEmpty {
                out.append(NSAttributedString(string: value + "\n", attributes: [.font: NSFont.systemFont(ofSize: 13), .foregroundColor: panelText, .paragraphStyle: inset]))
            }
        }
        out.append(NSAttributedString(string: "\n", attributes: [.font: mono, .paragraphStyle: tight]))
        return out
    }
    for (r, row) in rows.enumerated() {
        let cells = (0..<columns).map { i in wrappedCell(i < row.count ? row[i] : "", width: natural[i]) }
        let height = cells.map(\.count).max() ?? 1
        for line in 0..<height {
            var text = ""
            for i in 0..<columns {
                let piece = line < cells[i].count ? cells[i][line] : ""
                text += piece + String(repeating: " ", count: max(0, natural[i] - piece.count))
                if i < columns - 1 { text += " │ " }
            }
            out.append(NSAttributedString(string: text + "\n", attributes: [.font: r == 0 ? bold : mono, .foregroundColor: r == 0 ? toneGold : panelText, .paragraphStyle: tight]))
        }
        if r == 0 {
            let rule = natural.map { String(repeating: "─", count: $0) }.joined(separator: "─┼─")
            out.append(NSAttributedString(string: rule + "\n", attributes: [.font: mono, .foregroundColor: panelBorder, .paragraphStyle: tight]))
        }
    }
    out.append(NSAttributedString(string: "\n", attributes: [.font: mono, .paragraphStyle: tight]))
    return out
}

/// The font and colour one run of the parser's output asks for: block first (a heading is a heading), then
/// the inline traits, then the link.
func runAttributes(_ run: AttributedString.Runs.Run) -> [NSAttributedString.Key: Any] {
    var font = NSFont.systemFont(ofSize: 13)
    var colour = panelText
    var extra: [NSAttributedString.Key: Any] = [:]
    if let intent = run.presentationIntent {
        for component in intent.components {
            if case .header(let level) = component.kind {
                font = NSFont.systemFont(ofSize: level == 1 ? 21 : level == 2 ? 17 : level == 3 ? 14.5 : 13, weight: .bold)
                colour = level == 3 ? toneCyan : level <= 2 ? toneGold : panelText
            }
            if case .codeBlock = component.kind {
                font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
                colour = codeText
                extra[.backgroundColor] = codeBackground
            }
        }
    }
    let inline = run.inlinePresentationIntent ?? []
    if inline.contains(.code) {
        font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        colour = toneBlue
        extra[.backgroundColor] = codeBackground
    }
    if inline.contains(.stronglyEmphasized) { font = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask) }
    if inline.contains(.emphasized) { font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask) }
    if inline.contains(.strikethrough) { extra[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
    // A link is worth nothing if it does not look like one, and the delegate opens it on click.
    if let url = run.link {
        extra[.link] = url
        extra[.underlineStyle] = NSUnderlineStyle.single.rawValue
        colour = toneBlue
    }
    var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: colour]
    attributes.merge(extra) { $1 }
    return attributes
}

/// Paragraph shape, plus the two markers that only exist on screen: the bullet and the quote bar.
func dressBlock(_ body: NSMutableAttributedString, range: NSRange, intent: PresentationIntent?) {
    var kinds: [PresentationIntent.Kind] = []
    if let intent = intent { kinds = intent.components.map(\.kind) }
    let style = NSMutableParagraphStyle()
    style.lineSpacing = 2
    style.paragraphSpacing = 6
    var marker = ""
    var markerColour = toneMuted
    var depth = 0
    var quoted = false
    let ordered = kinds.contains { if case .orderedList = $0 { return true } else { return false } }
    for kind in kinds {
        switch kind {
        case .header: style.paragraphSpacing = 10; style.paragraphSpacingBefore = 8
        case .codeBlock: style.paragraphSpacing = 8; style.lineSpacing = 1
        case .blockQuote: quoted = true
        case .listItem(let ordinal): depth += 1; marker = ordered ? "\(ordinal). " : "• "
        case .thematicBreak: marker = "────────"
        default: break
        }
    }
    if quoted { marker = "▏ " + marker; markerColour = toneCoral }
    let indent = CGFloat(max(0, depth - 1)) * 16
    style.firstLineHeadIndent = indent
    style.headIndent = indent + (marker.isEmpty ? 0 : 16)
    if marker.isEmpty {
        body.addAttribute(.paragraphStyle, value: style, range: range)
        return
    }
    body.insert(NSAttributedString(string: marker, attributes: [.font: NSFont.systemFont(ofSize: 13), .foregroundColor: markerColour]), at: range.location)
    let grown = NSRange(location: range.location, length: range.length + (marker as NSString).length)
    body.addAttribute(.paragraphStyle, value: style, range: grown)
}

/// A regex replacement that never crashes on a bad pattern: what is not matched is simply left as it was.
func replacing(_ text: String, _ pattern: String, _ template: String) -> String {
    guard let regex = try? NSRegularExpression(pattern: pattern) else { return text }
    let range = NSRange(location: 0, length: (text as NSString).length)
    return regex.stringByReplacingMatches(in: text, range: range, withTemplate: template)
}

/// One line of Markdown with its HTML rewritten into the Markdown it stands for. A README's hero is raw
/// HTML — `<div align="center">`, badges made of `<a>` around `<img>`, `<br>` — and CommonMark hands HTML
/// through as literal text, which is exactly the source a reader does not want to read.
func normalizedLine(_ line: String) -> String {
    var text = line
    // A badge is a link around an image: one picture with its name, not the URL soup it is written as.
    text = replacing(text, #"\[!\[([^\]]*)\]\([^)]*\)\]\([^)]*\)"#, "🖼 $1")
    text = replacing(text, #"!\[([^\]]*)\]\(([^)]*)\)"#, "🖼 $1 ($2)")
    text = replacing(text, #"<a\s[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#, "[$2]($1)")
    text = replacing(text, #"<img\s[^>]*?alt="([^"]*)"[^>]*?src="([^"]*)"[^>]*?>"#, "🖼 $1")
    text = replacing(text, #"<img\s[^>]*?src="([^"]*)"[^>]*?alt="([^"]*)"[^>]*?>"#, "🖼 $2")
    text = replacing(text, #"<img\s[^>]*src="([^"]*)"[^>]*?>"#, "🖼 $1")
    text = replacing(text, #"(?i)<br\s*/?>"#, "\n")
    text = replacing(text, #"</?[a-zA-Z][^>]*>"#, "")
    for (entity, plain) in [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&nbsp;", " ")] {
        text = text.replacingOccurrences(of: entity, with: plain)
    }
    return text
}

/// The whole source with its HTML normalised, except inside fenced code: a fence that shows HTML must keep it.
func normalizedMarkdown(_ source: String) -> String {
    var out: [String] = []
    var fenced = false
    for line in source.components(separatedBy: "\n") {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        let isFence = trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~")
        if isFence { fenced.toggle() }
        out.append(fenced || isFence ? line : normalizedLine(line))
    }
    return out.joined(separator: "\n")
}

/// One piece of source with no table in it: parsed, then dressed.
func markdownChunk(_ chunk: String) -> NSAttributedString {
    let source = normalizedMarkdown(chunk)
    let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .full, failurePolicy: .returnPartiallyParsedIfPossible)
    guard let parsed = try? AttributedString(markdown: source, options: options) else {
        // Never an empty window and never a crash: what the parser cannot read shows as it was written.
        return NSAttributedString(string: chunk, attributes: [.font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular), .foregroundColor: codeText])
    }
    let body = NSMutableAttributedString()
    var blocks: [(range: NSRange, intent: PresentationIntent?)] = []
    // Foundation carries the block structure in `presentationIntent` only: the characters of a heading run
    // straight into the paragraph after it, so the line breaks a document needs are added here.
    let base = NSFont.systemFont(ofSize: 13)
    var previous: PresentationIntent?
    var start = 0
    var first = true
    for run in parsed.runs {
        let intent = run.presentationIntent
        if !first, intent != previous {
            body.append(NSAttributedString(string: "\n", attributes: [.font: base, .foregroundColor: panelText]))
            blocks.append((NSRange(location: start, length: body.length - start), previous))
            start = body.length
        }
        let piece = String(parsed[run.range].characters)
        body.append(NSAttributedString(string: piece, attributes: runAttributes(run)))
        previous = intent
        first = false
    }
    if !first {
        body.append(NSAttributedString(string: "\n", attributes: [.font: base, .foregroundColor: panelText]))
        blocks.append((NSRange(location: start, length: body.length - start), previous))
    }
    // Backwards, so inserting a bullet never moves a block this loop has still to touch.
    for block in blocks.reversed() { dressBlock(body, range: block.range, intent: block.intent) }
    return body
}

/// A markdown file as a document: Foundation parses CommonMark, this dresses it in the rail's palette and adds
/// the three things the parser flattens — tables, image placeholders and a header with the close mark.
func markdownBody(_ source: String, title: String, cut: Bool, width: CGFloat) -> NSAttributedString {
    let base = NSFont.systemFont(ofSize: 13)
    let out = NSMutableAttributedString()
    out.append(NSAttributedString(string: "◆ ", attributes: [.font: base, .foregroundColor: toneGold]))
    out.append(NSAttributedString(string: (title as NSString).lastPathComponent, attributes: [.font: base, .foregroundColor: toneCyan]))
    out.append(NSAttributedString(string: "  [", attributes: [.font: base, .foregroundColor: toneMuted]))
    if let close = URL(string: "gs3://close") {
        out.append(NSAttributedString(string: "✕", attributes: [.font: base, .foregroundColor: toneCoral, .link: close]))
    }
    out.append(NSAttributedString(string: "]\n\n", attributes: [.font: base, .foregroundColor: toneMuted]))
    let trimmed = source.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.isEmpty {
        out.append(NSAttributedString(string: "(archivo vacío)", attributes: [.font: base, .foregroundColor: toneDim]))
        return out
    }
    for segment in markdownSegments(trimmed) {
        switch segment {
        case .table(let rows): out.append(renderedTable(rows, width: width))
        case .text(let chunk): out.append(markdownChunk(chunk))
        }
    }
    if cut {
        out.append(NSAttributedString(string: "\n… sólo los primeros 256 KiB\n", attributes: [.font: NSFont.systemFont(ofSize: 12), .foregroundColor: toneDim]))
    }
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
    let markdown: Bool
    let cut: Bool
    var picture: NSImageView?
    var stamp: Date?

    init(text: String, rect: NSRect, path: String?, hud: String?, image: Bool, markdown: Bool, cut: Bool) {
        self.text = text
        self.rect = rect
        self.path = path
        self.hud = hud
        self.image = image
        self.markdown = markdown
        self.cut = cut
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
        window.title = image ? "gs3 · imagen" : markdown ? "gs3 · markdown" : "gs3 · commit"
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
        if image { imageContent(card) } else if markdown { markdownContent(card, window) } else { textContent(card, window) }
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

    /// A markdown file as a document. No horizontal scroller on purpose: prose wraps at the window width,
    /// which is the whole difference with the text mode, where a diff or a code line must keep its columns.
    func markdownContent(_ card: CardView, _ window: NSWindow) {
        let scroll = NSScrollView(frame: NSRect(x: 1, y: 1, width: rect.width - 2, height: rect.height - 2))
        scroll.autoresizingMask = [.width, .height]
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.drawsBackground = false
        scroll.backgroundColor = panelBackground
        let view = NSTextView(frame: NSRect(x: 0, y: 0, width: rect.width - 2, height: rect.height - 2))
        view.textContainerInset = NSSize(width: 18, height: 14)
        view.isEditable = false
        view.isSelectable = true
        view.drawsBackground = true
        view.backgroundColor = panelBackground
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        view.textContainer?.widthTracksTextView = true
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.containerSize = NSSize(width: rect.width - 40, height: CGFloat.greatestFiniteMagnitude)
        view.autoresizingMask = [.width, .height]
        view.textStorage?.setAttributedString(markdownBody(text, title: path ?? "", cut: cut, width: rect.width - 40))
        reloader = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            self?.reloadMarkdown()
        }
        scroll.documentView = view
        card.addSubview(scroll)
        view.delegate = self
        window.makeFirstResponder(view)
    }

    /// Follow the file, exactly like the text window: the rail rewrites it when the answer arrives.
    func reloadMarkdown() {
        guard let file = path, let fresh = try? String(contentsOfFile: file, encoding: .utf8), fresh != text else { return }
        text = fresh
        guard let scroll = window?.contentView?.subviews.first as? NSScrollView, let view = scroll.documentView as? NSTextView else { return }
        let top = scroll.contentView.bounds.origin
        view.textStorage?.setAttributedString(markdownBody(fresh, title: file, cut: cut, width: rect.width - 40))
        scroll.contentView.scroll(to: top)
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
        // A link the document carries is a link the reader asked to follow: the browser is the right place.
        if let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" || scheme == "mailto" {
            NSWorkspace.shared.open(url)
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
let delegate = Delegate(text: text, rect: rect, path: file, hud: hud, image: image, markdown: markdown, cut: cut)
app.delegate = delegate
NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
    if event.keyCode == 53 { exit(0) }  // Esc
    return event
}
app.run()
