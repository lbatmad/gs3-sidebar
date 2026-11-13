//! Syntax highlighting for what TREE opens. The rail already speaks 24-bit SGR — the diff window has parsed
//! it since the commit inspector existed — so the whole integration is a grammar engine that emits the same
//! bytes. The grammars are Sublime's, via `two-face` (the maintained replacement for syntect's own set): no
//! keyword lists to rot, and Rust's `keyword.control` here is the same data VS Code and Sublime use.
//!
//! Everything is coloured by *scope*, never by guessing: a word inside a string or a comment is whatever the
//! grammar says it is, which is the one thing a hand-written lexer gets wrong.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, ScopeSelectors, Style, StyleModifier, Theme, ThemeItem, ThemeSettings};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

const BACKGROUND: Color = Color { r: 0x0D, g: 0x16, b: 0x24, a: 0xFF };
const TEXT: Color = Color { r: 0xE2, g: 0xE8, b: 0xF0, a: 0xFF };
/// The rail's tones. A source file is one more card in the same window, so it cannot come in other colours.
const GOLD: (u8, u8, u8) = (0xFF, 0xD2, 0x3F);
const CYAN: (u8, u8, u8) = (0x06, 0xB6, 0xD4);
const MINT: (u8, u8, u8) = (0x10, 0xB9, 0x81);
const ORANGE: (u8, u8, u8) = (0xFB, 0xBF, 0x24);
const CORAL: (u8, u8, u8) = (0xFB, 0x71, 0x85);
const BLUE: (u8, u8, u8) = (0x38, 0xBD, 0xF8);
const MUTED: (u8, u8, u8) = (0x94, 0xA3, 0xB8);
const DIM: (u8, u8, u8) = (0x84, 0x94, 0xA8);

fn colour((r, g, b): (u8, u8, u8)) -> Color { Color { r, g, b, a: 0xFF } }

/// Scope selector, colour and weight. Ordered from broad to specific: a later selector wins where they
/// overlap, and every one of them comes from a name the grammars actually use.
const RULES: [(&str, (u8, u8, u8), bool, bool); 16] = [
    ("source, text, meta.paragraph", (0xE2, 0xE8, 0xF0), false, false),
    ("comment, punctuation.definition.comment", DIM, false, true),
    ("keyword, keyword.control, keyword.other, keyword.operator.new, storage, storage.modifier", GOLD, true, false),
    ("string, string.quoted, string.template, string.unquoted, punctuation.definition.string", MINT, false, false),
    ("constant.numeric, constant.other, constant.other.placeholder", ORANGE, false, false),
    ("constant.language, constant.character.escape, constant.character", CORAL, false, false),
    ("entity.name.function, support.function, variable.function, meta.function-call", BLUE, false, false),
    ("entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, entity.name.impl, support.type, support.class", CYAN, false, false),
    ("entity.name.tag, entity.other.attribute-name", CORAL, false, false),
    ("entity.name.constant, variable.other.constant", ORANGE, false, false),
    ("variable.parameter, variable.other, variable", (0xE2, 0xE8, 0xF0), false, false),
    ("punctuation, meta.brace, meta.delimiter, keyword.operator", MUTED, false, false),
    ("invalid, invalid.illegal, message.error", CORAL, false, false),
    ("markup.heading", GOLD, true, false),
    ("markup.bold", (0xE2, 0xE8, 0xF0), true, false),
    ("markup.italic", (0xE2, 0xE8, 0xF0), false, true),
];

/// Built once: the theme is a fixed list of selectors, and building it per preview would be waste. It is
/// a `Theme` built by hand — there is no `ThemeBuilder` in syntect 5 — so the selectors above are the data.
fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let scopes = RULES.iter().filter_map(|(selector, tone, bold, italic)| {
            let scopes = selector.parse::<ScopeSelectors>().ok()?;
            let font_style = if *bold { Some(FontStyle::BOLD) } else if *italic { Some(FontStyle::ITALIC) } else { None };
            Some(ThemeItem { scope: scopes, style: StyleModifier { foreground: Some(colour(*tone)), background: None, font_style } })
        }).collect();
        Theme {
            name: Some("gs3".to_string()),
            author: None,
            settings: ThemeSettings { foreground: Some(TEXT), background: Some(BACKGROUND), ..ThemeSettings::default() },
            scopes,
        }
    })
}

/// Loading the ~200 grammars costs more than any single preview: once per process, lazily.
fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// The grammar that will colour this file: by name first, then by its first line (a `#!/bin/bash` script has
/// no extension to go by). `None` means the file is plain text and the window shows it as it is.
fn syntax_for<'a>(set: &'a SyntaxSet, path: &str, body: &str) -> Option<&'a SyntaxReference> {
    set.find_syntax_for_file(path).ok().flatten()
        .filter(|s| s.name != "Plain Text")
        .or_else(|| set.find_syntax_by_first_line(body.lines().next().unwrap_or_default()))
}

/// What the window's header calls this file's language ("Rust", "TypeScript"), or `None` when there is
/// nothing to highlight.
pub fn language(path: &str, body: &str) -> Option<String> {
    syntax_for(syntaxes(), path, body).map(|s| s.name.clone())
}

/// The file as the SGR the window already parses: one reset, the 24-bit colour, and bold/italic where the
/// theme asks for them. Colour is per line with the grammar's own state carried across, so a string or a
/// block comment that spans lines keeps colouring the lines it covers.
pub fn sgr(path: &str, body: &str) -> String {
    let set = syntaxes();
    let Some(syntax) = syntax_for(set, path, body) else { return body.to_string() };
    let mut highlighter = HighlightLines::new(syntax, theme());
    let mut out = String::with_capacity(body.len() + body.len() / 3);
    for line in LinesWithEndings::from(body) {
        match highlighter.highlight_line(line, set) {
            Ok(ranges) => out.push_str(&escaped(&ranges)),
            // A grammar that fails on one line must not cost the whole file: the line goes out as it is.
            Err(_) => out.push_str(line),
        }
    }
    out
}

/// The exact subset of SGR the panel implements: reset, 24-bit foreground, bold and italic. Anything else
/// would be silently dropped on the other side, so nothing else is emitted.
fn escaped(ranges: &[(Style, &str)]) -> String {
    let mut out = String::new();
    for (style, text) in ranges {
        if text.is_empty() { continue }
        let (r, g, b) = (style.foreground.r, style.foreground.g, style.foreground.b);
        out.push_str(&format!("\u{1b}[0m\u{1b}[38;2;{r};{g};{b}m"));
        if style.font_style.contains(FontStyle::BOLD) { out.push_str("\u{1b}[1m"); }
        if style.font_style.contains(FontStyle::ITALIC) { out.push_str("\u{1b}[3m"); }
        out.push_str(text);
    }
    out.push_str("\u{1b}[0m");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The (colour, text) runs of an SGR string, so a test can ask what a word was painted with.
    fn runs(text: &str) -> Vec<(Option<(u8, u8, u8)>, String)> {
        let mut out: Vec<(Option<(u8, u8, u8)>, String)> = Vec::new();
        let mut tone: Option<(u8, u8, u8)> = None;
        let mut buffer = String::new();
        let mut i = 0;
        while i < text.len() {
            if text[i..].starts_with("\u{1b}[") {
                if let Some(end) = text[i..].find('m') {
                    if !buffer.is_empty() { out.push((tone, std::mem::take(&mut buffer))) }
                    let body = &text[i + 2..i + end];
                    let parts: Vec<u32> = body.split(';').map_while(|p| p.parse().ok()).collect();
                    if parts.len() == 5 && parts[0] == 38 && parts[1] == 2 {
                        tone = Some((parts[2] as u8, parts[3] as u8, parts[4] as u8));
                    } else if parts.first() == Some(&0) {
                        tone = None;
                    }
                    i += end + 1;
                    continue;
                }
            }
            let ch = text[i..].chars().next().unwrap();
            buffer.push(ch);
            i += ch.len_utf8();
        }
        if !buffer.is_empty() { out.push((tone, buffer)) }
        out
    }

    /// The colour of every run the needle touches, in order: a phrase like `// cuenta` arrives as several
    /// runs, and asking "what colour is this word" means the runs it covers, not one run that contains it.
    fn covering(output: &str, needle: &str) -> Vec<Option<(u8, u8, u8)>> {
        let runs = runs(output);
        let whole: String = runs.iter().map(|(_, text)| text.as_str()).collect();
        let Some(start) = whole.find(needle) else { return Vec::new() };
        let end = start + needle.len();
        let mut offset = 0;
        let mut tones = Vec::new();
        for (tone, text) in &runs {
            let (from, to) = (offset, offset + text.len());
            if from < end && to > start { tones.push(*tone) }
            offset = to;
        }
        tones
    }

    fn painted(output: &str, needle: &str) -> Option<(u8, u8, u8)> {
        covering(output, needle).first().copied().flatten()
    }

    #[test]
    fn a_keyword_a_type_a_string_a_number_and_a_comment_are_told_apart() {
        let code = "fn main() -> u32 {\n    // cuenta\n    let n: u32 = 1_000;\n    let s = \"hola\";\n    n\n}\n";
        let out = sgr("a.rs", code);
        assert_eq!(painted(&out, "fn"), Some(GOLD), "la palabra clave va en dorado: {out}");
        assert_eq!(painted(&out, "// cuenta"), Some(DIM), "el comentario va en gris");
        assert_eq!(painted(&out, "\"hola\""), Some(MINT), "el string va en verde");
        assert_eq!(painted(&out, "1_000"), Some(ORANGE), "el número va en ámbar");
        assert!(out.contains("\u{1b}[1m"), "la palabra clave va en negrita, y el panel la honra");
        assert!(out.contains("\u{1b}[3m"), "el comentario va en itálica");
    }

    #[test]
    fn a_keyword_inside_a_string_or_a_comment_stays_what_it_is() {
        let out = sgr("a.rs", "let s = \"fn if let\";\n// fn if let\n");
        // The string's own colour covers the words inside it: nothing a lexer's keyword table would get right.
        assert_eq!(painted(&out, "\"fn if let\""), Some(MINT));
        assert_eq!(painted(&out, "// fn if let"), Some(DIM));
        assert_eq!(covering(&out, "fn if let").len(), 1, "el string es un solo tramo verde, no palabras clave adentro");
        assert!(covering(&out, "// fn if let").iter().all(|t| *t == Some(DIM)), "el comentario tampoco deja escapar la palabra clave");
    }

    #[test]
    fn the_grammar_state_survives_across_lines() {
        let out = sgr("a.rs", "/* uno\nfn dentro del comentario\ndos */\nfn fuera() {}\n");
        assert_eq!(covering(&out, "fn dentro del comentario"), vec![Some(DIM)], "un comentario de bloque cubre sus líneas");
        assert_eq!(painted(&out, "fn fuera"), Some(GOLD), "y se cierra donde el lenguaje dice");
    }

    #[test]
    fn a_language_is_recognised_by_name_and_by_its_first_line() {
        assert_eq!(language("a.rs", "").as_deref(), Some("Rust"));
        assert_eq!(language("a.py", "").as_deref(), Some("Python"));
        assert_eq!(language("script", "#!/bin/bash\nls\n").as_deref(), Some("Bourne Again Shell (bash)"));
        assert_eq!(language("notas.txt", "hola"), None, "un .txt no se resalta");
        assert_eq!(language("sin-extension", "hola"), None);
    }

    #[test]
    fn a_file_with_no_grammar_comes_out_exactly_as_it_went_in() {
        let body = "hola, qué tal\n";
        assert_eq!(sgr("notas.txt", body), body);
        assert_eq!(sgr("sin-extension", body), body);
    }

    #[test]
    fn what_is_not_a_known_sgr_sequence_is_never_emitted() {
        let out = sgr("a.rs", "fn main() {}\n");
        for part in out.split("\u{1b}[").skip(1) {
            let code = part.split('m').next().unwrap_or_default();
            assert!(
                matches!(code, "0" | "1" | "3") || (code.starts_with("38;2;") && code.split(';').count() == 5),
                "solo el subconjunto que el panel entiende: {code}"
            );
        }
    }
}
