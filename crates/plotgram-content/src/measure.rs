//! Heuristic measurement: ContentDoc + MeasureParams -> ContentLayout.
//!
//! Sole geometry writer for content blocks (ADR-005 §2). No font files are
//! opened (ADR-005 §3): widths come from codepoint-class em factors below,
//! calibrated against the assumed fonts by
//! `scripts/calibrate_content_measure.py` (which mirrors `char_class`).
//! Wrapping (ADR-005 §2: the wrap policy is fixed in the measuring phase) is
//! greedy line filling over atoms: wide glyphs break anywhere, Latin-ish
//! words move whole, oversized tokens are hard-split. Alignment and
//! `max_lines` truncation (trailing `…`) are also decided here — downstream
//! consumers only ever see final geometry.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::ast::{Block, ContentDoc, Line, RunStyle};

// ── inputs ─────────────────────────────────────────────────────────────────

/// Horizontal alignment of visual lines within the content box. The box is
/// the ink extent (widest line defines `width`); narrower lines shift inside
/// it. Placing the box itself inside a node is the renderer's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Everything that influences content geometry. This crate never defaults
/// from a theme — the orchestration layer compiles and passes these in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasureParams {
    pub font_size: f64,
    /// Line box height = `font_size * line_height`.
    pub line_height: f64,
    /// Width-estimation assumption, recorded for `emit_svg` to echo back.
    pub font_family: String,
    /// Vertical gap between blocks.
    pub paragraph_gap: f64,
    /// Left indent of list item content; the marker sits at x = 0.
    pub list_indent: f64,
    pub rule_thickness: f64,
    /// Vertical whitespace above + below a rule (split evenly).
    pub rule_gap: f64,
    /// Wrap budget for the whole content block. Paragraphs wrap within
    /// `max_width`; list content wraps within `max_width - list_indent` and
    /// continuation lines keep the indent (marker only on the first visual
    /// line). Hard-split guarantees ≥ 1 char per line, so a budget narrower
    /// than one glyph may still overflow. `None` = natural single-line width.
    pub max_width: Option<f64>,
    /// Horizontal alignment of lines within the content box.
    #[serde(default)]
    pub align: Align,
    /// Keep at most this many visual lines. The last kept line gets a
    /// trailing `…` re-fit into `max_width`; blocks below the cut are
    /// dropped. `None` = no truncation. Clamped to ≥ 1.
    #[serde(default)]
    pub max_lines: Option<usize>,
}

// ── outputs ────────────────────────────────────────────────────────────────

/// Line-box / run geometry + total size. Written once by `measure`; consumed
/// by layout (size) and by `emit_svg` (expansion only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentLayout {
    pub width: f64,
    pub height: f64,
    pub lines: Vec<LineBox>,
    /// Rule center-line y positions; rules span the full content width.
    pub rules: Vec<f64>,
    /// Frozen typography, echoed by emit so paint cannot alter geometry.
    pub font_size: f64,
    pub font_family: String,
    pub rule_thickness: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineBox {
    pub top: f64,
    pub height: f64,
    /// Text baseline (absolute y), fixed here so emit makes zero decisions.
    pub baseline: f64,
    pub runs: Vec<RunBox>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunBox {
    pub x: f64,
    pub width: f64,
    pub text: String,
    pub style: RunStyle,
}

// ── width heuristic (calibration target) ──────────────────────────────────

/// Codepoint classes for width estimation. Must stay in sync with
/// `char_class()` in `scripts/calibrate_content_measure.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    Wide,
    Upper,
    Lower,
    Digit,
    PunctNarrow,
    Space,
    Other,
}

pub fn char_class(c: char) -> CharClass {
    let cp = c as u32;
    // Pragmatic East-Asian-wide ranges (full EAW table lives in the
    // calibration script; keep these ranges mirrored).
    let wide = matches!(cp,
        0x1100..=0x115F   // Hangul Jamo
        | 0x2E80..=0x303F // CJK radicals, symbols & punctuation
        | 0x3040..=0x30FF // Hiragana / Katakana
        | 0x3400..=0x4DBF // CJK Ext A
        | 0x4E00..=0x9FFF // CJK Unified
        | 0xAC00..=0xD7AF // Hangul syllables
        | 0xF900..=0xFAFF // CJK Compatibility
        | 0xFE30..=0xFE4F // CJK compat forms
        | 0xFF00..=0xFF60 // Fullwidth forms
        | 0x20000..=0x2FA1F // CJK Ext B+
    );
    // EAW-ambiguous glyphs that are full-width (~1em) in Noto Sans CJK, the
    // assumed measuring font: middle dot, curly quotes, em dash, bullet,
    // ellipsis, arrows, plus wide ASCII symbols (@ 0.95em, % 0.92em).
    let wide_in_noto = matches!(cp,
        0x00B7            // ·
        | 0x2014          // —
        | 0x2018..=0x201F // ‘’“”…
        | 0x2022          // •
        | 0x2026          // …
        | 0x2190..=0x21FF // arrows
    ) || matches!(c, '@' | '%');
    if wide || wide_in_noto {
        CharClass::Wide
    } else if c == ' ' {
        CharClass::Space
    } else if c.is_ascii_uppercase() {
        CharClass::Upper
    } else if c.is_ascii_lowercase() {
        CharClass::Lower
    } else if c.is_ascii_digit() {
        CharClass::Digit
    } else if matches!(c, '.' | ',' | ':' | ';' | '!' | '\'' | '|') {
        CharClass::PunctNarrow
    } else {
        CharClass::Other
    }
}

/// Advance width in em. Calibrated 2026-07-30 against Noto Sans CJK SC
/// Regular/Bold + Menlo via `calibrate_content_measure.py calibrate`
/// (measured mean + ~3% safety; residual error is absorbed by padding).
fn em_factor(class: CharClass, style: RunStyle) -> f64 {
    if style == RunStyle::Code {
        // Monospace cell (Menlo 0.602em measured); wide glyphs fall back to
        // the CJK font at ~1em.
        return match class {
            CharClass::Wide => 1.0,
            _ => 0.62,
        };
    }
    // (regular, bold) per class; emph is synthetic oblique = regular advance.
    let (regular, bold) = match class {
        CharClass::Wide => (1.0, 1.0),          // measured 0.995 / 0.995
        CharClass::Upper => (0.66, 0.69),       // measured 0.636 / 0.669
        CharClass::Lower => (0.55, 0.59),       // measured 0.531 / 0.566
        CharClass::Digit => (0.58, 0.61),       // measured 0.560 / 0.590
        CharClass::PunctNarrow => (0.30, 0.35), // measured 0.284 / 0.331
        CharClass::Space => (0.23, 0.24),       // measured 0.220 / 0.230
        CharClass::Other => (0.65, 0.68),       // measured mean 0.511, max 0.95
    };
    if style == RunStyle::Strong {
        bold
    } else {
        regular
    }
}

/// Estimated advance width of `text` at `font_size`. Public for the
/// estimates-dump tooling (`examples/dump_estimates.rs`).
pub fn estimate_text_width(text: &str, style: RunStyle, font_size: f64) -> f64 {
    text.chars().map(|c| em_factor(char_class(c), style) * font_size).sum()
}

// ── measure ────────────────────────────────────────────────────────────────

/// Ascent as a fraction of font_size (baseline = top + font_size * ASCENT).
const ASCENT: f64 = 0.8;
/// Unordered list marker glyph (spec §2.2: normalized bullet).
const BULLET: &str = "•";

pub fn measure(doc: &ContentDoc, p: &MeasureParams) -> ContentLayout {
    let line_h = p.font_size * p.line_height;
    // Budgets derived once here: the list continuation indent is a decision
    // of the measuring phase, not of any downstream consumer.
    let para_budget = p.max_width.unwrap_or(f64::INFINITY);
    let list_budget = p.max_width.map_or(f64::INFINITY, |w| (w - p.list_indent).max(p.font_size));
    let mut lines: Vec<LineBox> = Vec::new();
    let mut rules: Vec<f64> = Vec::new();
    let mut y = 0.0;
    let mut width: f64 = 0.0;

    for (idx, block) in doc.blocks.iter().enumerate() {
        if idx > 0 {
            y += p.paragraph_gap;
        }
        match block {
            Block::Paragraph { lines: para } => {
                for line in para {
                    push_wrapped(line, 0.0, para_budget, None, p, line_h, &mut y, &mut width, &mut lines);
                }
            }
            Block::List { ordered, items } => {
                for item in items {
                    let marker = match (ordered, item.number) {
                        (true, Some(n)) => format!("{n}."),
                        _ => BULLET.to_string(),
                    };
                    push_wrapped(
                        &item.line,
                        p.list_indent,
                        list_budget,
                        Some(&marker),
                        p,
                        line_h,
                        &mut y,
                        &mut width,
                        &mut lines,
                    );
                }
            }
            Block::Rule => {
                y += p.rule_gap / 2.0;
                rules.push(y + p.rule_thickness / 2.0);
                y += p.rule_thickness + p.rule_gap / 2.0;
            }
        }
    }

    // Truncation, then alignment: both are measuring-phase decisions taken
    // after all lines exist, because both need the final line set / width.
    if let Some(maxl) = p.max_lines {
        let keep = maxl.max(1);
        if lines.len() > keep {
            lines.truncate(keep);
            let cutoff = lines.last().map_or(0.0, |lb| lb.top + lb.height);
            rules.retain(|&r| r < cutoff);
            y = cutoff;
            add_ellipsis(lines.last_mut().expect("keep >= 1"), para_budget, p.font_size);
            width = lines.iter().fold(0.0, |acc, lb| acc.max(line_width(lb)));
        }
    }
    let align_f = match p.align {
        Align::Left => 0.0,
        Align::Center => 0.5,
        Align::Right => 1.0,
    };
    if align_f > 0.0 {
        for lb in &mut lines {
            let dx = (width - line_width(lb)) * align_f;
            if dx > EPS {
                for r in &mut lb.runs {
                    r.x += dx;
                }
            }
        }
    }

    ContentLayout {
        width,
        height: y,
        lines,
        rules,
        font_size: p.font_size,
        font_family: p.font_family.clone(),
        rule_thickness: p.rule_thickness,
    }
}

/// Wrap one logical line into visual lines and append them. The marker (list
/// bullet / number) is prepended to the first visual line only, at x = 0.
#[allow(clippy::too_many_arguments)]
fn push_wrapped(
    line: &Line,
    x0: f64,
    budget: f64,
    marker: Option<&str>,
    p: &MeasureParams,
    line_h: f64,
    y: &mut f64,
    width: &mut f64,
    lines: &mut Vec<LineBox>,
) {
    let rows = wrap_atoms(atomize(line, p.font_size), budget, p.font_size);
    for (i, row) in rows.iter().enumerate() {
        let mut lb = assemble(row, x0, *y, line_h, p.font_size);
        if i == 0 {
            if let Some(m) = marker {
                lb.runs.insert(
                    0,
                    RunBox {
                        x: 0.0,
                        width: estimate_text_width(m, RunStyle::Plain, p.font_size),
                        text: m.to_string(),
                        style: RunStyle::Plain,
                    },
                );
            }
        }
        *width = width.max(line_width(&lb));
        lines.push(lb);
        *y += line_h;
    }
}

fn line_width(lb: &LineBox) -> f64 {
    lb.runs.iter().fold(0.0, |acc, r| acc.max(r.x + r.width))
}

// ── truncation ─────────────────────────────────────────────────────────────

/// Marker glyph appended when `max_lines` cuts content.
const ELLIPSIS: char = '…';

/// Pop the last glyph of the line (shrinking / removing its run). Returns
/// false when the line has no runs left.
fn pop_last_char(lb: &mut LineBox, font_size: f64) -> bool {
    let Some(last) = lb.runs.last_mut() else { return false };
    match last.text.pop() {
        Some(c) => {
            last.width -= em_factor(char_class(c), last.style) * font_size;
            if last.text.is_empty() {
                lb.runs.pop();
            }
            true
        }
        None => {
            lb.runs.pop();
            true
        }
    }
}

/// Append `…` to the truncated last line: drop trailing spaces, then pop
/// glyphs until the ellipsis fits the budget again (≥ 1 glyph kept, same
/// overflow caveat as hard-split). The ellipsis is a Plain run so its width
/// matches the `…` calibration class.
fn add_ellipsis(lb: &mut LineBox, budget: f64, font_size: f64) {
    let ell_w = em_factor(char_class(ELLIPSIS), RunStyle::Plain) * font_size;
    let glyphs = |lb: &LineBox| lb.runs.iter().map(|r| r.text.chars().count()).sum::<usize>();
    let trailing_space = |lb: &LineBox| lb.runs.last().is_some_and(|r| r.text.ends_with(' '));
    while trailing_space(lb) {
        pop_last_char(lb, font_size);
    }
    while line_width(lb) + ell_w > budget + EPS && glyphs(lb) > 1 {
        pop_last_char(lb, font_size);
        while trailing_space(lb) {
            pop_last_char(lb, font_size);
        }
    }
    match lb.runs.last_mut() {
        Some(last) if last.style == RunStyle::Plain => {
            last.text.push(ELLIPSIS);
            last.width += ell_w;
        }
        Some(last) => {
            let x = last.x + last.width;
            lb.runs.push(RunBox { x, width: ell_w, text: ELLIPSIS.to_string(), style: RunStyle::Plain });
        }
        None => {
            lb.runs.push(RunBox { x: 0.0, width: ell_w, text: ELLIPSIS.to_string(), style: RunStyle::Plain });
        }
    }
}

// ── wrapping ───────────────────────────────────────────────────────────────

const EPS: f64 = 1e-6;

/// Wrap unit. Wide (CJK-ish) glyphs break anywhere so each is its own atom;
/// Latin-ish words move as a whole; consecutive spaces form one atom that is
/// droppable at line boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomKind {
    Word,
    Space,
    Wide,
}

#[derive(Debug, Clone)]
struct Atom {
    text: String,
    width: f64,
    style: RunStyle,
    kind: AtomKind,
}

/// Kinsoku: CJK closing punctuation must not start a line — glue it to the
/// previous atom so it wraps together with the preceding glyph.
fn glues_to_prev(c: char) -> bool {
    matches!(c, '，' | '。' | '、' | '：' | '；' | '！' | '？' | '）' | '】' | '」' | '』' | '〉' | '》' | '…' | '’' | '”')
}

/// Kinsoku: CJK opening punctuation must not end a line — glue the next
/// glyph onto it.
fn glues_to_next(c: char) -> bool {
    matches!(c, '（' | '【' | '「' | '『' | '〈' | '《' | '‘' | '“')
}

fn atomize(line: &Line, font_size: f64) -> VecDeque<Atom> {
    let mut atoms: VecDeque<Atom> = VecDeque::new();
    for run in &line.runs {
        for c in run.text.chars() {
            let class = char_class(c);
            let kind = match class {
                CharClass::Wide => AtomKind::Wide,
                CharClass::Space => AtomKind::Space,
                _ => AtomKind::Word,
            };
            let w = em_factor(class, run.style) * font_size;
            let joined = match atoms.back_mut() {
                Some(last) if last.style == run.style && kind != AtomKind::Space => {
                    let same_word = last.kind == kind && kind != AtomKind::Wide;
                    let closing = kind == AtomKind::Wide
                        && glues_to_prev(c)
                        && last.kind != AtomKind::Space;
                    let after_opening = last.text.chars().next_back().is_some_and(glues_to_next);
                    if same_word || closing || after_opening {
                        last.text.push(c);
                        last.width += w;
                        if after_opening {
                            // "（R" continues as a word / "（验" as wide, so the
                            // following glyphs keep joining per normal rules.
                            last.kind = kind;
                        }
                        true
                    } else {
                        false
                    }
                }
                Some(last)
                    if last.style == run.style
                        && last.kind == kind
                        && kind == AtomKind::Space =>
                {
                    last.text.push(c);
                    last.width += w;
                    true
                }
                _ => false,
            };
            if !joined {
                atoms.push_back(Atom { text: c.to_string(), width: w, style: run.style, kind });
            }
        }
    }
    atoms
}

/// Greedy line filling. Continuation lines drop leading spaces; lines closed
/// by a wrap drop trailing spaces (the final line keeps authored spaces so an
/// unwrapped measurement is byte-identical to the pre-wrap behavior). An
/// oversized atom on an empty line is hard-split with ≥ 1 char per line.
fn wrap_atoms(mut atoms: VecDeque<Atom>, budget: f64, font_size: f64) -> Vec<Vec<Atom>> {
    let mut out: Vec<Vec<Atom>> = Vec::new();
    let mut cur: Vec<Atom> = Vec::new();
    let mut x = 0.0;
    while let Some(atom) = atoms.pop_front() {
        if atom.kind == AtomKind::Space && cur.is_empty() && !out.is_empty() {
            continue; // leading space on a continuation line
        }
        if x + atom.width <= budget + EPS {
            x += atom.width;
            cur.push(atom);
        } else if cur.is_empty() {
            // Nothing on this line yet and the atom alone overflows: hard-split.
            let (head, tail) = split_atom(atom, budget, font_size);
            cur.push(head);
            if let Some(t) = tail {
                atoms.push_front(t);
            }
            close_row(&mut out, &mut cur, &mut x);
        } else {
            atoms.push_front(atom);
            close_row(&mut out, &mut cur, &mut x);
        }
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

fn close_row(out: &mut Vec<Vec<Atom>>, cur: &mut Vec<Atom>, x: &mut f64) {
    while cur.last().is_some_and(|a| a.kind == AtomKind::Space) {
        cur.pop();
    }
    out.push(std::mem::take(cur));
    *x = 0.0;
}

/// Split an oversized atom so the head fills the budget (≥ 1 char). Returns
/// `(atom, None)` when the atom is a single char and cannot be split.
fn split_atom(atom: Atom, budget: f64, font_size: f64) -> (Atom, Option<Atom>) {
    let chars: Vec<char> = atom.text.chars().collect();
    let mut cut = 0;
    let mut w = 0.0;
    for &c in &chars {
        let cw = em_factor(char_class(c), atom.style) * font_size;
        if cut > 0 && w + cw > budget + EPS {
            break;
        }
        w += cw;
        cut += 1;
    }
    if cut >= chars.len() {
        return (atom, None);
    }
    let head: String = chars[..cut].iter().collect();
    let tail: String = chars[cut..].iter().collect();
    let head_w = estimate_text_width(&head, atom.style, font_size);
    let tail_w = estimate_text_width(&tail, atom.style, font_size);
    (
        Atom { text: head, width: head_w, style: atom.style, kind: atom.kind },
        Some(Atom { text: tail, width: tail_w, style: atom.style, kind: atom.kind }),
    )
}

/// Rebuild RunBoxes from a wrapped row, merging adjacent same-style atoms so
/// run boundaries match the parser's `merge_adjacent` invariant.
fn assemble(row: &[Atom], x0: f64, top: f64, line_h: f64, font_size: f64) -> LineBox {
    let mut runs: Vec<RunBox> = Vec::new();
    let mut x = x0;
    for atom in row {
        match runs.last_mut() {
            Some(last) if last.style == atom.style => {
                last.text.push_str(&atom.text);
                last.width += atom.width;
            }
            _ => runs.push(RunBox { x, width: atom.width, text: atom.text.clone(), style: atom.style }),
        }
        x += atom.width;
    }
    LineBox { top, height: line_h, baseline: top + font_size * ASCENT, runs }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn params(font_size: f64) -> MeasureParams {
        MeasureParams {
            font_size,
            line_height: 1.4,
            font_family: "Noto Sans CJK SC".into(),
            paragraph_gap: 6.0,
            list_indent: 18.0,
            rule_thickness: 1.0,
            rule_gap: 8.0,
            max_width: None,
            align: Align::Left,
            max_lines: None,
        }
    }

    #[test]
    fn measure_observable_geometry() {
        let text = "**标题**\n---\n- 第一项\n- item two";
        let cl = measure(&parse(text), &params(14.0));

        // 3 rendered lines (1 paragraph line + 2 list items), 1 rule.
        assert_eq!(cl.lines.len(), 3);
        assert_eq!(cl.rules.len(), 1);
        // List content starts at the fixed indent, marker at x = 0.
        let item = &cl.lines[1];
        assert_eq!(item.runs[0].x, 0.0);
        assert_eq!(item.runs[1].x, 18.0);
        // Total size covers the widest line and the stacked height.
        assert!(cl.width >= line_width(&cl.lines[2]));
        assert!(cl.height > cl.lines[2].top);

        // Determinism: identical input -> identical layout.
        assert_eq!(cl, measure(&parse(text), &params(14.0)));

        // ADR-005 test requirement: a bigger font_size must grow preferred size.
        let big = measure(&parse(text), &params(28.0));
        assert!(big.width > cl.width && big.height > cl.height);
        // Text widths scale linearly with font_size.
        let (a, b) = (&cl.lines[0].runs[0], &big.lines[0].runs[0]);
        assert!((b.width - a.width * 2.0).abs() < 1e-9);
    }

    /// Flattened run text of every visual line, spaces stripped (wrapping is
    /// allowed to drop spaces at line boundaries, never any other ink).
    fn flat_ink(cl: &ContentLayout) -> String {
        cl.lines
            .iter()
            .flat_map(|l| &l.runs)
            .flat_map(|r| r.text.chars())
            .filter(|c| *c != ' ')
            .collect()
    }

    #[test]
    fn wrap_table() {
        let eps = 1e-6;
        // (name, text, max_width)
        let cases: &[(&str, &str, f64)] = &[
            ("cjk_breaks_anywhere", "订单服务处理下单主链路校验库存", 100.0),
            ("latin_words_move_whole", "check inventory levels now", 90.0),
            ("long_token_hard_split", "supercalifragilisticexpialidocious", 60.0),
            ("mixed_cjk_latin", "HTTP 504 网关超时后重试三次", 110.0),
            ("styled_runs_survive", "**重试策略**指数退避加 `jitter` 随机扰动", 120.0),
            // Greedy per-char breaking would start line 2 with "，".
            ("kinsoku_no_leading_punct", "一二三四五六七，八九十", 100.0),
            ("list_continuation_indent", "- 校验库存并落库并发消息去重处理", 120.0),
        ];
        for &(name, text, max_width) in cases {
            let mut p = params(14.0);
            p.max_width = Some(max_width);
            let cl = measure(&parse(text), &p);

            // Wrapping actually happened and every visual line fits the budget.
            assert!(cl.lines.len() >= 2, "{name}: expected wrapping, got {} lines", cl.lines.len());
            for (i, lb) in cl.lines.iter().enumerate() {
                assert!(
                    line_width(lb) <= max_width + eps,
                    "{name}: line {i} width {} exceeds budget {max_width}",
                    line_width(lb)
                );
                assert!(!lb.runs.is_empty(), "{name}: line {i} is empty");
                // Kinsoku: closing punctuation never starts a visual line,
                // opening punctuation never ends one.
                let first = lb.runs.first().and_then(|r| r.text.chars().next()).unwrap();
                let last = lb.runs.last().and_then(|r| r.text.chars().next_back()).unwrap();
                assert!(!glues_to_prev(first), "{name}: line {i} starts with {first:?}");
                assert!(!glues_to_next(last), "{name}: line {i} ends with {last:?}");
            }
            assert!(cl.width <= max_width + eps, "{name}: total width exceeds max_width");

            // No ink lost or reordered vs the unwrapped measurement.
            let unwrapped = measure(&parse(text), &params(14.0));
            assert_eq!(flat_ink(&cl), flat_ink(&unwrapped), "{name}: ink mismatch");

            // Determinism.
            assert_eq!(cl, measure(&parse(text), &p), "{name}: nondeterministic");
        }

        // Words move whole: rejoining lines with single spaces restores the text.
        let mut p = params(14.0);
        p.max_width = Some(90.0);
        let cl = measure(&parse("check inventory levels now"), &p);
        let joined = cl
            .lines
            .iter()
            .map(|l| l.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(joined, "check inventory levels now");

        // List continuation lines keep the indent; marker only on line 1.
        p.max_width = Some(120.0);
        let cl = measure(&parse("- 校验库存并落库并发消息去重处理"), &p);
        assert_eq!(cl.lines[0].runs[0].text, "•");
        assert_eq!(cl.lines[0].runs[0].x, 0.0);
        assert_eq!(cl.lines[0].runs[1].x, 18.0);
        for (i, lb) in cl.lines.iter().enumerate().skip(1) {
            assert_eq!(lb.runs[0].x, 18.0, "continuation line {i} must keep list indent");
            assert_ne!(lb.runs[0].text, "•", "continuation line {i} must not repeat marker");
        }

        // max_width = None keeps each logical line on one visual line.
        let cl = measure(&parse("订单服务处理下单主链路校验库存"), &params(14.0));
        assert_eq!(cl.lines.len(), 1);
    }

    #[test]
    fn align_and_truncate() {
        let eps = 1e-9;
        // Alignment shifts narrower lines inside the content box; it never
        // resizes anything (width/height/run widths identical to Left).
        let text = "**发布确认**\n等待人工审批中处理";
        let base = measure(&parse(text), &params(14.0));
        for (align, f) in [(Align::Left, 0.0), (Align::Center, 0.5), (Align::Right, 1.0)] {
            let mut p = params(14.0);
            p.align = align;
            let cl = measure(&parse(text), &p);
            assert_eq!((cl.width, cl.height), (base.width, base.height), "{align:?}: box resized");
            for (la, lb) in cl.lines.iter().zip(&base.lines) {
                let dx = (cl.width - line_width(lb)) * f;
                for (ra, rb) in la.runs.iter().zip(&lb.runs) {
                    assert!((ra.x - (rb.x + dx)).abs() < eps, "{align:?}: bad shift");
                    assert_eq!(ra.width, rb.width, "{align:?}: run resized");
                }
            }
            if align == Align::Right {
                // Every line is flush right against the content box.
                for lb in &cl.lines {
                    assert!((line_width(lb) - cl.width).abs() < eps, "right: not flush");
                }
            }
            assert_eq!(cl, measure(&parse(text), &p), "{align:?}: nondeterministic");
        }

        // Truncation: keep max_lines, last line ends with … and re-fits the
        // budget, height stops at the cut, rules above the cut survive.
        let long = "**支付回调**\n---\n收到网关回调后先验签再幂等落库失败进入重试队列并告警通知值班";
        let mut p = params(14.0);
        p.max_width = Some(160.0);
        p.max_lines = Some(3);
        let cl = measure(&parse(long), &p);
        assert_eq!(cl.lines.len(), 3);
        let last = cl.lines.last().unwrap();
        assert!(last.runs.last().unwrap().text.ends_with('…'), "missing ellipsis");
        assert!(line_width(last) <= 160.0 + 1e-6, "ellipsis line overflows budget");
        assert!((cl.height - (last.top + last.height)).abs() < eps, "height beyond the cut");
        assert_eq!(cl.rules.len(), 1, "rule above the cut must survive");
        assert_eq!(cl, measure(&parse(long), &p), "truncate: nondeterministic");

        // No truncation when the content already fits the line budget.
        p.max_lines = Some(50);
        let full = measure(&parse(long), &p);
        assert!(full.lines.len() > 3);
        assert!(!full.lines.last().unwrap().runs.last().unwrap().text.ends_with('…'));
    }
}
