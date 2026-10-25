// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Minimal Markdown renderer producing an egui [`LayoutJob`].
//!
//! The official client renders message bodies with a full Markdown engine.
//! Re-implementing it entirely would be pointless, so this module covers the
//! subset that actually appears in chat and maps it onto egui text sections:
//!
//! * inline `**bold**`, `*italic*` / `_italic_`, `__underline__`, `~~strike~~`,
//!   `` `inline code` `` and links `[label](url)`;
//! * fenced code blocks, block quotes, `#` headings and list bullets;
//! * Fluxer markup leftovers: `<@id>`, `<#id>`, `<:name:id>`.
//!
//! Rendering is deliberately forgiving: unmatched markers stay literal.

use eframe::egui::{self, Color32, FontId, Stroke, TextFormat, text::LayoutJob};

use crate::theme;

/// Line height factor used by the official stylesheet (`line-height: 1.375`).
pub const LINE_HEIGHT: f32 = 1.375;

#[derive(Clone, Copy, Default)]
struct Format {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    code: bool,
    link: bool,
    heading: bool,
}

/// Parses `content` and returns a ready-to-use layout job.
pub fn render(content: &str, size: f32, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut in_code_block = false;
    let mut first_line = true;

    for line in content.split('\n') {
        let trimmed = line.trim_start();
        let newline = !first_line;
        first_line = false;

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            if newline {
                append_newline(&mut job, size, color);
            }
            continue;
        }

        if in_code_block {
            if newline {
                append_newline(&mut job, size, color);
            }
            let fmt = Format {
                code: true,
                ..Format::default()
            };
            job.append(line, 0.0, format_for(size * 0.94, color, &fmt));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("> ") {
            if newline {
                append_newline(&mut job, size, color);
            }
            let fmt = Format {
                italic: true,
                ..Format::default()
            };
            job.append(
                "▏ ",
                0.0,
                format_for(size, theme::TEXT_FAINT, &Format::default()),
            );
            push_inline(&mut job, rest, size, theme::TEXT_MUTED, fmt);
            continue;
        }

        let (body, size_mult, heading) = if let Some(rest) = trimmed.strip_prefix("### ") {
            (rest, 1.05, true)
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            (rest, 1.15, true)
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            (rest, 1.3, true)
        } else if let Some(rest) = trimmed.strip_prefix("- ") {
            if newline {
                append_newline(&mut job, size, color);
            }
            job.append(
                "• ",
                0.0,
                format_for(size, theme::TEXT_MUTED, &Format::default()),
            );
            push_inline(&mut job, rest, size, color, Format::default());
            continue;
        } else {
            (line, 1.0, false)
        };

        if newline {
            append_newline(&mut job, size, color);
        }
        let fmt = Format {
            bold: heading,
            heading,
            ..Format::default()
        };
        push_inline(&mut job, body, size * size_mult, color, fmt);
    }

    job
}

fn append_newline(job: &mut LayoutJob, size: f32, color: Color32) {
    job.append("\n", 0.0, format_for(size, color, &Format::default()));
}

/// Appends one line of inline Markdown.
fn push_inline(job: &mut LayoutJob, line: &str, size: f32, color: Color32, base: Format) {
    let chars: Vec<char> = line.chars().collect();
    let mut fmt = base;
    let mut buffer = String::new();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !buffer.is_empty() {
                job.append(&buffer, 0.0, format_for(size, color, &fmt));
                buffer.clear();
            }
        };
    }

    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\\' => {
                if let Some(&escaped) = chars.get(i + 1) {
                    buffer.push(escaped);
                    i += 2;
                } else {
                    buffer.push('\\');
                    i += 1;
                }
            }
            '`' => {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                    flush!();
                    let code: String = chars[i + 1..i + 1 + end].iter().collect();
                    let code_fmt = Format {
                        code: true,
                        ..Format::default()
                    };
                    job.append(&code, 0.0, format_for(size * 0.94, color, &code_fmt));
                    i += end + 2;
                } else {
                    buffer.push('`');
                    i += 1;
                }
            }
            '*' | '_' | '~' => {
                let run = chars[i..].iter().take_while(|&&c| c == ch).count();
                let len = if ch == '~' {
                    if run >= 2 { 2 } else { 0 }
                } else if run >= 2 {
                    2
                } else {
                    1
                };
                if len == 0 {
                    buffer.push(ch);
                    i += 1;
                    continue;
                }
                // Le marqueur est actif : c'est une fermeture.
                let active = match (ch, len) {
                    ('~', _) => fmt.strike,
                    ('_', 2) => fmt.underline,
                    ('_', _) => fmt.italic,
                    ('*', 2) => fmt.bold,
                    ('*', _) => fmt.italic,
                    _ => false,
                };
                if active {
                    flush!();
                    match (ch, len) {
                        ('~', _) => fmt.strike = false,
                        ('_', 2) => fmt.underline = false,
                        ('_', _) => fmt.italic = false,
                        ('*', 2) => fmt.bold = false,
                        ('*', _) => fmt.italic = false,
                        _ => {}
                    }
                    i += len;
                    continue;
                }

                // Ouverture : on vérifie qu'une fermeture existe plus loin
                // pour ne pas basculer sur un astérisque isolé.
                let mut close_at = None;
                let mut j = i + len;
                while j < chars.len() {
                    if chars[j] == ch {
                        let close_run = chars[j..].iter().take_while(|&&c| c == ch).count();
                        if close_run >= len {
                            close_at = Some(j);
                            break;
                        }
                    }
                    j += 1;
                }
                let Some(_close_at) = close_at else {
                    buffer.push(ch);
                    i += 1;
                    continue;
                };

                flush!();
                match (ch, len) {
                    ('~', _) => fmt.strike = true,
                    ('_', 2) => fmt.underline = true,
                    ('_', _) => fmt.italic = true,
                    ('*', 2) => fmt.bold = true,
                    ('*', _) => fmt.italic = true,
                    _ => {}
                }
                // On ne saute que le marqueur d'ouverture : le contenu suit.
                i += len;
            }
            '[' => {
                if let Some((label, url, end)) = parse_link(&chars, i) {
                    flush!();
                    let link_fmt = Format { link: true, ..fmt };
                    job.append(&label, 0.0, format_for(size, theme::LINK, &link_fmt));
                    job.append(" ", 0.0, format_for(size, color, &fmt));
                    let _ = url;
                    i = end;
                } else {
                    buffer.push('[');
                    i += 1;
                }
            }
            '<' => {
                if let Some((consumed, replacement)) = parse_angle(&chars, i) {
                    buffer.push_str(&replacement);
                    i += consumed;
                } else {
                    buffer.push('<');
                    i += 1;
                }
            }
            _ => {
                buffer.push(ch);
                i += 1;
            }
        }
    }
    flush!();
}

/// Parses `[label](url)` starting at `start`. Returns the label, URL and the
/// index just after `)`.
fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let mut i = start + 1;
    let mut label = String::new();
    while i < chars.len() {
        match chars[i] {
            ']' => break,
            c => label.push(c),
        }
        i += 1;
    }
    if i + 1 >= chars.len() || chars[i] != ']' || chars[i + 1] != '(' {
        return None;
    }
    let mut j = i + 2;
    let mut url = String::new();
    while j < chars.len() {
        match chars[j] {
            ')' => return Some((label, url, j + 1)),
            c => url.push(c),
        }
        j += 1;
    }
    None
}

/// Handles `<@id>`, `<#id>` and `<:name:id>` leftovers.
fn parse_angle(chars: &[char], start: usize) -> Option<(usize, String)> {
    let mut end = start + 1;
    while end < chars.len() && chars[end] != '>' {
        end += 1;
    }
    if end >= chars.len() {
        return None;
    }
    let raw: String = chars[start + 1..end].iter().collect();
    let consumed = end - start + 1;

    if let Some(name) = raw.strip_prefix('@') {
        let name = name.trim_start_matches('&');
        let label = if name.chars().all(|c| c.is_ascii_digit()) {
            "@utilisateur".to_string()
        } else {
            format!("@{name}")
        };
        return Some((consumed, label));
    }
    if let Some(name) = raw.strip_prefix('#') {
        let label = if name.chars().all(|c| c.is_ascii_digit()) {
            "#salon".to_string()
        } else {
            format!("#{name}")
        };
        return Some((consumed, label));
    }
    if let Some(rest) = raw.strip_prefix(':') {
        let name = rest.split(':').next().unwrap_or("emoji");
        return Some((consumed, format!(":{name}:")));
    }
    None
}

fn format_for(size: f32, color: Color32, fmt: &Format) -> TextFormat {
    let font = if fmt.code {
        FontId::monospace(size * 0.94)
    } else {
        match (fmt.bold, fmt.italic) {
            (true, true) => FontId::new(
                size,
                egui::FontFamily::Name(crate::fonts::BOLD_ITALIC.into()),
            ),
            (true, false) => crate::fonts::bold(size),
            (false, true) => crate::fonts::italic(size),
            (false, false) => FontId::proportional(size),
        }
    };
    let line_height = Some((size * LINE_HEIGHT).max(size + 2.0));
    let text_color = if fmt.link {
        theme::LINK
    } else if fmt.code {
        Color32::from_rgb(0xeb, 0xeb, 0xef)
    } else if fmt.heading {
        theme::TEXT
    } else {
        color
    };

    TextFormat {
        font_id: font,
        color: text_color,
        background: if fmt.code {
            theme::CODE_BG
        } else {
            Color32::TRANSPARENT
        },
        italics: fmt.italic,
        underline: if fmt.link || fmt.underline {
            Stroke::new(1.0, text_color)
        } else {
            Stroke::NONE
        },
        strikethrough: if fmt.strike {
            Stroke::new(1.0, text_color)
        } else {
            Stroke::NONE
        },
        line_height,
        ..Default::default()
    }
}

/// Strips Markdown to plain text (used for previews and notifications).
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' | '_' | '~' | '`' => {}
            '[' => {
                let mut label = String::new();
                let mut closed = false;
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == ']' {
                        closed = true;
                        break;
                    }
                    label.push(next);
                }
                if closed {
                    if matches!(chars.peek(), Some('(')) {
                        for next in chars.by_ref() {
                            if next == ')' {
                                break;
                            }
                        }
                    }
                    out.push_str(&label);
                } else {
                    out.push('[');
                    out.push_str(&label);
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_plain_text() {
        assert_eq!(plain("bonjour **tout** le monde"), "bonjour tout le monde");
    }

    #[test]
    fn strips_links() {
        assert_eq!(plain("voir [ici](https://x.y) ok"), "voir ici ok");
    }

    #[test]
    fn renders_without_panic() {
        let job = render(
            "**gras** *italique* `code` ~~barré~~ <@123>\n> citation\n```\ncode\n```",
            15.0,
            theme::TEXT,
        );
        assert!(!job.sections.is_empty());
        assert!(job.text.contains("gras"));
        assert!(job.text.contains("code"));
    }
}
