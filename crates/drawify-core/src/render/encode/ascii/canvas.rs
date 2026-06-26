use super::{PADDING, SCALE_X, SCALE_Y};
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Cell {
    Empty,
    Char(char),
    WideCont,
}

pub(super) struct DisplayCanvas {
    pub(super) rows: Vec<Vec<Cell>>,
}

pub(super) struct GridRect {
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) w: usize,
    pub(super) h: usize,
}

pub(super) struct GridMapper {
    title_rows: usize,
}

impl GridMapper {
    pub(super) fn new(has_title: bool) -> Self {
        Self {
            title_rows: if has_title { 2 } else { 0 },
        }
    }

    pub(super) fn to_grid(&self, x: f64, y: f64) -> (usize, usize) {
        let gx = (x / SCALE_X).round() as usize + PADDING;
        let gy = (y / SCALE_Y).round() as usize + PADDING + self.title_rows;
        (gx, gy)
    }
}

impl DisplayCanvas {
    pub(super) fn new(width: usize, height: usize) -> Self {
        Self {
            rows: vec![vec![Cell::Empty; width]; height],
        }
    }

    pub(super) fn ensure(&mut self, y: usize, x_end: usize) {
        while self.rows.len() <= y {
            let w = self.rows.first().map(|r| r.len()).unwrap_or(80);
            self.rows.push(vec![Cell::Empty; w]);
        }
        for row in &mut self.rows {
            while row.len() <= x_end {
                row.push(Cell::Empty);
            }
        }
    }

    pub(super) fn clear_span(&mut self, y: usize, x: usize, width: usize) {
        self.ensure(y, x + width.saturating_sub(1));
        for i in 0..width {
            self.rows[y][x + i] = Cell::Empty;
        }
    }

    pub(super) fn set_char(&mut self, x: usize, y: usize, ch: char) {
        let w = char_width(ch);
        self.ensure(y, x + w.saturating_sub(1));
        self.clear_span(y, x, w);
        self.rows[y][x] = Cell::Char(ch);
        for i in 1..w {
            self.rows[y][x + i] = Cell::WideCont;
        }
    }

    pub(super) fn write_text(&mut self, x: usize, y: usize, text: &str) {
        let mut col = x;
        for ch in text.chars() {
            self.set_char(col, y, ch);
            col += char_width(ch);
        }
    }

    pub(super) fn is_interior(&self, x: usize, y: usize, rects: &[GridRect]) -> bool {
        rects.iter().any(|r| {
            x > r.x && x < r.x + r.w.saturating_sub(1) && y > r.y && y < r.y + r.h.saturating_sub(1)
        })
    }

    pub(super) fn to_string(&self) -> String {
        // Convert each row to a trimmed string
        let lines: Vec<String> = self
            .rows
            .iter()
            .map(|row| {
                let mut line = String::new();
                for cell in row.iter() {
                    match cell {
                        Cell::Empty => line.push(' '),
                        Cell::Char(ch) => line.push(*ch),
                        Cell::WideCont => {}
                    }
                }
                line.trim_end().to_string()
            })
            .collect();

        // Find the max display width across all non-empty lines
        let max_width = lines
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| text_width(l))
            .max()
            .unwrap_or(0);

        // Pad all non-empty lines to the same display width so that terminal
        // line-wrapping doesn't misalign the right borders.
        lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let pad = max_width.saturating_sub(text_width(&line));
                if pad > 0 {
                    let mut padded = line;
                    padded.push_str(&" ".repeat(pad));
                    padded
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(super) fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0).max(1)
}

pub(super) fn text_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub(super) fn truncate_string(s: &str, max_width: usize) -> String {
    if text_width(s) <= max_width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = char_width(ch);
        if w + cw > max_width.saturating_sub(1) {
            out.push('…');
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}
