#![allow(dead_code)]

use crossterm::{
    cursor, execute, queue,
    style::{self, Attribute, Attributes, Color},
};
use std::cmp::Ordering;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::utils;

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum Attr {
    Bold = 0b0000_0001,
    Dim = 0b0000_0010,
    Italic = 0b0000_0100,
    Underline = 0b0000_1000,
    Reverse = 0b0001_0000,
    Hide = 0b0010_0000,
    CrossedOut = 0b0100_0000,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Default)]
pub struct Attrs(pub u8);

impl Attrs {
    pub fn set(&mut self, attr: Attr) {
        self.0 |= attr as u8;
    }

    pub fn unset(&mut self, attr: Attr) {
        self.0 ^= attr as u8;
    }

    pub fn has(&self, attr: Attr) -> bool {
        self.0 & attr as u8 != 0
    }
}

impl std::ops::BitAnd<Attrs> for Attrs {
    type Output = Self;

    fn bitand(self, rhs: Attrs) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::BitOr<Attrs> for Attrs {
    type Output = Self;

    fn bitor(self, rhs: Attrs) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitXor<Attrs> for Attrs {
    type Output = Self;

    fn bitxor(self, rhs: Attrs) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl std::ops::Sub<Attrs> for Attrs {
    type Output = Self;

    fn sub(self, rhs: Attrs) -> Self::Output {
        Self(self.0 ^ rhs.0 & self.0)
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
}

impl Style {
    pub fn new(fg: Color, bg: Color, attrs: Attrs) -> Self {
        Self { fg, bg, attrs }
    }

    pub fn set_fg(&mut self, fg: Color) -> &mut Self {
        self.fg = fg;
        self
    }

    pub fn set_bg(&mut self, bg: Color) -> &mut Self {
        self.bg = bg;
        self
    }

    pub fn set_attrs(&mut self, attrs: Attrs) -> &mut Self {
        self.attrs = attrs;
        self
    }

    pub fn parse_ansi_code(&mut self, s: &str) {
        let s = s.split(';').collect::<Vec<&str>>();
        let mut nums: Vec<u8> = Vec::new();
        for i in s {
            nums.push(i.parse::<u8>().unwrap_or_default());
        }
        let mut index = 0;
        while index < nums.len() {
            let n = nums[index];
            match n {
                0 => self.reset(),
                1 => self.attrs.set(Attr::Bold),
                2 => self.attrs.set(Attr::Dim),
                3 => self.attrs.set(Attr::Italic),
                4 => self.attrs.set(Attr::Underline),
                5 | 6 => log::info!("Not implemented for `SlowBlink` and `RapidBlink`"),
                7 => self.attrs.set(Attr::Reverse),
                8 => self.attrs.set(Attr::Hide),
                9 => self.attrs.set(Attr::CrossedOut),
                30 => self.fg = Color::Black,
                31 => self.fg = Color::DarkRed,
                32 => self.fg = Color::DarkGreen,
                33 => self.fg = Color::DarkYellow,
                34 => self.fg = Color::DarkBlue,
                35 => self.fg = Color::DarkMagenta,
                36 => self.fg = Color::DarkCyan,
                37 => self.fg = Color::Grey,
                40 => self.bg = Color::Black,
                41 => self.bg = Color::DarkRed,
                42 => self.bg = Color::DarkGreen,
                43 => self.bg = Color::DarkYellow,
                44 => self.bg = Color::DarkBlue,
                45 => self.bg = Color::DarkMagenta,
                46 => self.bg = Color::DarkCyan,
                47 => self.bg = Color::Grey,
                90 => self.fg = Color::DarkGrey,
                91 => self.fg = Color::Red,
                92 => self.fg = Color::Green,
                93 => self.fg = Color::Yellow,
                94 => self.fg = Color::Blue,
                95 => self.fg = Color::Magenta,
                96 => self.fg = Color::Cyan,
                97 => self.fg = Color::White,
                100 => self.bg = Color::DarkGrey,
                101 => self.bg = Color::Red,
                102 => self.bg = Color::Green,
                103 => self.bg = Color::Yellow,
                104 => self.bg = Color::Blue,
                105 => self.bg = Color::Magenta,
                106 => self.bg = Color::Cyan,
                107 => self.bg = Color::White,
                38 | 48 => {
                    let selfc = if nums[index] == 38 {
                        &mut self.fg
                    } else {
                        &mut self.bg
                    };
                    if index + 2 < nums.len() && nums[index + 1] == 5 {
                        *selfc = Color::AnsiValue(nums[index + 2]);
                        index += 2;
                    } else if index + 4 < nums.len() && nums[index + 1] == 2 {
                        *selfc = Color::Rgb {
                            r: nums[index + 2],
                            g: nums[index + 3],
                            b: nums[index + 4],
                        };
                        index += 4;
                    } else {
                        log::error!("Unsupported color format");
                    }
                }
                39 => self.fg = Color::Reset,
                49 => self.bg = Color::Reset,
                _ => log::error!("Unsupported ansi code"),
            }
            index += 1;
        }
    }

    pub fn reset(&mut self) {
        self.fg = Color::Reset;
        self.bg = Color::Reset;
        self.attrs = Attrs::default();
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fg: Color::Reset,
            bg: Color::Reset,
            attrs: Attrs::default(),
        }
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct Cell {
    c: char,
    s: Style,
}

impl Cell {
    pub fn new(c: char, s: Style) -> Self {
        Self { c, s }
    }

    pub fn set_char(&mut self, c: char) -> &mut Self {
        self.c = c;
        self
    }

    pub fn set_fg(&mut self, fg: Color) -> &mut Self {
        self.s.set_fg(fg);
        self
    }

    pub fn set_bg(&mut self, bg: Color) -> &mut Self {
        self.s.set_bg(bg);
        self
    }

    pub fn set_attrs(&mut self, attrs: Attrs) -> &mut Self {
        self.s.attrs = attrs;
        self
    }

    pub fn set_style(&mut self, s: &Style) -> &mut Self {
        self.s.set_fg(s.fg);
        self.s.set_bg(s.bg);
        self.s.set_attrs(s.attrs);
        self
    }

    pub fn reset(&mut self) {
        self.c = ' ';
        self.s = Style::default();
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            s: Style::default(),
        }
    }
}

pub struct Buffer {
    pub size: (u16, u16),
    cells: Vec<Cell>,
    prev_cells: Vec<Cell>,
}

impl Buffer {
    pub fn new() -> Self {
        let (cols, rows) = utils::terminal_size();
        let cells = vec![Cell::default(); (cols * rows) as usize];
        let prev_cells = cells.clone();
        Self {
            size: (cols, rows),
            cells,
            prev_cells,
        }
    }

    pub fn get(&mut self, c: u16, r: u16) -> &Cell {
        &self.cells[(r * self.size.0 + c) as usize]
    }

    pub fn get_mut(&mut self, c: u16, r: u16) -> &mut Cell {
        &mut self.cells[(r * self.size.0 + c) as usize]
    }

    pub fn set_content(&mut self, c: u16, r: u16, ch: char, s: &Style) {
        if ((r * self.size.0 + c) as usize) < self.cells.len() {
            self.get_mut(c, r).set_char(ch).set_style(s);
        }
    }

    /*
     * str len: 5
     * 0123456789
     * ----------
     *   ^   |
     *   |---->
     *   c
     */
    pub fn set_line(&mut self, c: u16, r: u16, win_cols: u16, str: &str, s: &Style) {
        if win_cols == 0 {
            return;
        }
        let mut pos = 0;
        for ch in str.chars() {
            if (pos as u16) >= win_cols - 1 {
                break;
            }
            self.set_content(c + pos as u16, r, ch, s);
            pos += ch.width().unwrap_or_default();
        }
    }

    /*
     * str len: 5
     * 0123456789
     * ----------
     *    |   ^|
     *   <----||
     *         c
     */
    pub fn set_line_from_right(&mut self, c: u16, r: u16, str: &str, s: &Style) {
        if c >= str.width() as u16 {
            let mut pos = c - str.width() as u16;
            for ch in str.chars() {
                self.set_content(pos, r, ch, s);
                pos += ch.width().unwrap_or_default() as u16;
            }
        }
    }

    pub fn reset(&mut self) {
        for c in &mut self.cells {
            c.reset();
        }
        self.prev_cells = self.cells.clone();
    }

    pub fn resize(&mut self) {
        let size = utils::terminal_size();
        self.resize_x_y(size.0, size.1);
    }

    pub fn resize_x_y(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
        let len = (self.size.0 * self.size.1) as usize;
        let prev_len = self.cells.len();
        match len.cmp(&prev_len) {
            Ordering::Less => self.cells.truncate(len),
            Ordering::Greater => self.cells.resize_with(len, Default::default),
            Ordering::Equal => {}
        }
        self.reset();
    }

    pub fn draw(&mut self, out: &mut Box<dyn std::io::Write>) -> std::io::Result<()> {
        let updates = self.diff();
        let mut last_style = Style::default();
        execute!(out, cursor::SavePosition)?;
        for t in updates {
            queue!(out, cursor::MoveTo(t.0, t.1))?;
            if last_style.fg != t.2.s.fg {
                queue!(out, style::SetForegroundColor(t.2.s.fg))?;
                last_style.set_fg(t.2.s.fg);
            }

            if last_style.bg != t.2.s.bg {
                queue!(out, style::SetBackgroundColor(t.2.s.bg))?;
                last_style.set_bg(t.2.s.bg);
            }

            if last_style.attrs != t.2.s.attrs {
                let remove = last_style.attrs - t.2.s.attrs;
                let add = t.2.s.attrs - last_style.attrs;
                // NOTE: remove
                if remove.has(Attr::Bold) {
                    queue!(out, style::SetAttribute(Attribute::NormalIntensity))?;
                    if t.2.s.attrs.has(Attr::Dim) {
                        queue!(out, style::SetAttribute(Attribute::Dim))?;
                    }
                }
                if remove.has(Attr::Dim) {
                    queue!(out, style::SetAttribute(Attribute::NormalIntensity))?;
                    if t.2.s.attrs.has(Attr::Bold) {
                        queue!(out, style::SetAttribute(Attribute::Bold))?;
                    }
                }
                if remove.has(Attr::Italic) {
                    queue!(out, style::SetAttribute(Attribute::NoItalic))?;
                }
                if remove.has(Attr::Underline) {
                    queue!(out, style::SetAttribute(Attribute::NoUnderline))?;
                }
                if remove.has(Attr::Reverse) {
                    queue!(out, style::SetAttribute(Attribute::NoReverse))?;
                }
                if remove.has(Attr::Hide) {
                    queue!(out, style::SetAttribute(Attribute::NoHidden))?;
                }
                if remove.has(Attr::CrossedOut) {
                    queue!(out, style::SetAttribute(Attribute::NotCrossedOut))?;
                }

                // NOTE: add
                if add.has(Attr::Bold) {
                    queue!(out, style::SetAttribute(Attribute::Bold))?;
                }
                if add.has(Attr::Dim) {
                    queue!(out, style::SetAttribute(Attribute::Dim))?;
                }
                if add.has(Attr::Italic) {
                    queue!(out, style::SetAttribute(Attribute::Italic))?;
                }
                if add.has(Attr::Underline) {
                    queue!(out, style::SetAttribute(Attribute::Underlined))?;
                }
                if add.has(Attr::Reverse) {
                    queue!(out, style::SetAttribute(Attribute::Reverse))?;
                }
                if add.has(Attr::Hide) {
                    queue!(out, style::SetAttribute(Attribute::Hidden))?;
                }
                if add.has(Attr::CrossedOut) {
                    queue!(out, style::SetAttribute(Attribute::CrossedOut))?;
                }

                last_style.set_attrs(t.2.s.attrs);
            }

            queue!(out, style::Print(t.2.c))?;
        }

        queue!(
            out,
            style::ResetColor,
            style::SetAttributes(Attributes::default())
        )?;
        execute!(out, cursor::RestorePosition)?;
        out.flush()?;
        self.prev_cells = self.cells.clone();
        Ok(())
    }

    pub fn diff(&mut self) -> Vec<(u16, u16, &Cell)> {
        let cols = self.size.0;
        let mut updates: Vec<(u16, u16, &Cell)> = vec![];
        let mut invalidated: usize = 0;
        let mut to_skip: usize = 0;

        for (i, (current, previous)) in self.cells.iter().zip(self.prev_cells.iter()).enumerate() {
            if (current != previous || invalidated > 0) && to_skip == 0 {
                let c = i as u16 % cols;
                let r = i as u16 / cols;
                updates.push((c, r, &self.cells[i]));
            }

            to_skip = current.c.width().unwrap_or_default().saturating_sub(1);

            let affected_width = std::cmp::max(
                current.c.width().unwrap_or_default(),
                previous.c.width().unwrap_or_default(),
            );
            invalidated = std::cmp::max(affected_width, invalidated).saturating_sub(1);
        }

        updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff() {
        let mut b = Buffer::new();
        b.set_content(0, 0, 'c', &Style::default());
        b.set_content(1, 0, 'a', &Style::default());
        b.set_content(2, 0, '杰', &Style::default());
        b.set_content(4, 0, '😀', &Style::default());
        b.set_content(6, 0, '漢', &Style::default());
        println!("{:?}", b.diff());
    }

    #[test]
    fn test_parse_ansi_code() {
        let mut st = Style::default();
        st.parse_ansi_code("38;2;10;20;255");
        let mut attrs = Attrs::default();
        assert_eq!(
            st,
            Style {
                fg: Color::Rgb {
                    r: 10,
                    g: 20,
                    b: 255
                },
                bg: Color::Reset,
                attrs: attrs,
            }
        );
        st.parse_ansi_code("1");
        attrs.set(Attr::Bold);
        assert_eq!(
            st,
            Style {
                fg: Color::Rgb {
                    r: 10,
                    g: 20,
                    b: 255
                },
                bg: Color::Reset,
                attrs: attrs,
            }
        );
        st.parse_ansi_code("48;5;34");
        assert_eq!(
            st,
            Style {
                fg: Color::Rgb {
                    r: 10,
                    g: 20,
                    b: 255
                },
                bg: Color::AnsiValue(34),
                attrs: attrs,
            }
        );
        st.parse_ansi_code("0");
        assert_eq!(st, Style::default());
    }
}
