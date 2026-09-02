//! Minimal raw-I²C SSD1306 (128×32) text driver — for the diag menu + stress status.
//!
//! Deliberately **not** built on the `ssd1306`/`embedded-graphics` crate stack (that stack is
//! FROZEN — see NOTES "Design stance"). This is a few hundred bytes of direct register writes plus a
//! 5×7 font: the retro-diag-ROM way, and the pattern all *new* display work uses. Horizontal
//! addressing mode; each glyph is 5 column bytes (bit0 = top row) + a 1 px gap → 6 px advance, so a
//! 128 px line holds 21 chars and a 128×32 panel shows 4 lines (pages).
//!
//! It shares the bus with everything else at 0x3C; the POST verdict display (`oled.rs`) still uses
//! the frozen crate — the two coexist because only one is active per UI mode.

use crate::i2c;
use msp430fr2476::Peripherals;

const ADDR: u8 = 0x3C;
const CMD: u8 = 0x00; // control byte: the following bytes are commands
const DATA: u8 = 0x40; // control byte: the following bytes are GDDRAM data

/// Init a 128×32 panel (charge-pump on, horizontal addressing). Returns false if the panel NACKs.
pub fn init(p: &Peripherals) -> bool {
    i2c::write(
        p,
        ADDR,
        &[
            CMD, 0xAE, // display off
            0xD5, 0x80, // clock divide/oscillator
            0xA8, 0x1F, // multiplex ratio = 31 (32 rows)
            0xD3, 0x00, // display offset 0
            0x40, // start line 0
            0x8D, 0x14, // charge pump on
            0x20, 0x00, // memory addressing = horizontal
            0xA1, // segment remap (col 127 -> SEG0)
            0xC8, // COM scan direction remapped
            0xDA, 0x02, // COM pins config for 128×32
            0x81, 0x8F, // contrast
            0xD9, 0xF1, // pre-charge
            0xDB, 0x40, // VCOMH deselect
            0xA4, // resume to RAM content
            0xA6, // normal (not inverted)
            0x2E, // deactivate scroll
            0xAF, // display on
        ],
    )
}

/// Fill every pixel with `byte` (0x00 = all off, 0xFF = all on). Used for the boot self-test — a
/// fully-lit panel is the unambiguous "the raw driver works" signal.
pub fn fill(p: &Peripherals, byte: u8) {
    let mut buf = [byte; 129];
    buf[0] = DATA; // control byte, not pixel data
    for page in 0..4u8 {
        window(p, page, 0, 127);
        i2c::write(p, ADDR, &buf);
    }
}

/// Set the write window to one page and a column span (horizontal addressing auto-increments).
fn window(p: &Peripherals, page: u8, col0: u8, col1: u8) {
    i2c::write(p, ADDR, &[CMD, 0x22, page, page, 0x21, col0, col1]);
}

/// Blank the whole panel.
pub fn clear(p: &Peripherals) {
    let mut buf = [0u8; 129];
    buf[0] = DATA;
    for page in 0..4u8 {
        window(p, page, 0, 127);
        i2c::write(p, ADDR, &buf);
    }
}

/// Clear from `col` to the end of `page` — used to wipe the tail of a variable-width line so a
/// shrinking value can't leave stale glyphs behind.
pub fn clear_eol(p: &Peripherals, page: u8, col: u8) {
    if col > 127 {
        return;
    }
    window(p, page, col, 127);
    let mut b = [0u8; 129]; // b[0] = DATA control byte; the rest are blank columns
    b[0] = DATA;
    let n = 1 + (128 - col as usize);
    i2c::write(p, ADDR, &b[..n]);
}

/// Draw one glyph (6 px cell) at (page, col).
fn put(p: &Peripherals, page: u8, col: u8, g: &[u8; 5]) {
    window(p, page, col, col + 5);
    i2c::write(p, ADDR, &[DATA, g[0], g[1], g[2], g[3], g[4], 0x00]);
}

/// Render `s` at (page, col); returns the next free column. Clips at the right edge.
pub fn text(p: &Peripherals, page: u8, col: u8, s: &str) -> u8 {
    let mut c = col;
    for &ch in s.as_bytes() {
        if c > 121 {
            break;
        }
        put(p, page, c, &glyph(ch));
        c += 6;
    }
    c
}

/// Render `v` in decimal (no leading zeros) at (page, col); returns the next free column.
pub fn num(p: &Peripherals, page: u8, col: u8, v: u32) -> u8 {
    let mut tmp = [0u8; 10];
    let mut n = 0usize;
    let mut val = v;
    if val == 0 {
        tmp[0] = b'0';
        n = 1;
    }
    while val > 0 {
        tmp[n] = b'0' + (val % 10) as u8;
        val /= 10;
        n += 1;
    }
    let mut c = col;
    while n > 0 {
        n -= 1;
        if c > 121 {
            break;
        }
        put(p, page, c, &glyph(tmp[n]));
        c += 6;
    }
    c
}

// 5×7 font (column-major, bit0 = top row) for the glyphs the diag UI needs. Standard glcdfont
// values. Anything unmapped renders blank.
#[rustfmt::skip]
static DIGITS: [[u8; 5]; 10] = [
    [0x3E, 0x51, 0x49, 0x45, 0x3E], // 0
    [0x00, 0x42, 0x7F, 0x40, 0x00], // 1
    [0x42, 0x61, 0x51, 0x49, 0x46], // 2
    [0x21, 0x41, 0x45, 0x4B, 0x31], // 3
    [0x18, 0x14, 0x12, 0x7F, 0x10], // 4
    [0x27, 0x45, 0x45, 0x45, 0x39], // 5
    [0x3C, 0x4A, 0x49, 0x49, 0x30], // 6
    [0x01, 0x71, 0x09, 0x05, 0x03], // 7
    [0x36, 0x49, 0x49, 0x49, 0x36], // 8
    [0x06, 0x49, 0x49, 0x29, 0x1E], // 9
];

#[rustfmt::skip]
static ALPHA: [[u8; 5]; 26] = [
    [0x7E, 0x11, 0x11, 0x11, 0x7E], // A
    [0x7F, 0x49, 0x49, 0x49, 0x36], // B
    [0x3E, 0x41, 0x41, 0x41, 0x22], // C
    [0x7F, 0x41, 0x41, 0x22, 0x1C], // D
    [0x7F, 0x49, 0x49, 0x49, 0x41], // E
    [0x7F, 0x09, 0x09, 0x09, 0x01], // F
    [0x3E, 0x41, 0x49, 0x49, 0x7A], // G
    [0x7F, 0x08, 0x08, 0x08, 0x7F], // H
    [0x00, 0x41, 0x7F, 0x41, 0x00], // I
    [0x20, 0x40, 0x41, 0x3F, 0x01], // J
    [0x7F, 0x08, 0x14, 0x22, 0x41], // K
    [0x7F, 0x40, 0x40, 0x40, 0x40], // L
    [0x7F, 0x02, 0x0C, 0x02, 0x7F], // M
    [0x7F, 0x04, 0x08, 0x10, 0x7F], // N
    [0x3E, 0x41, 0x41, 0x41, 0x3E], // O
    [0x7F, 0x09, 0x09, 0x09, 0x06], // P
    [0x3E, 0x41, 0x51, 0x21, 0x5E], // Q
    [0x7F, 0x09, 0x19, 0x29, 0x46], // R
    [0x46, 0x49, 0x49, 0x49, 0x31], // S
    [0x01, 0x01, 0x7F, 0x01, 0x01], // T
    [0x3F, 0x40, 0x40, 0x40, 0x3F], // U
    [0x1F, 0x20, 0x40, 0x20, 0x1F], // V
    [0x7F, 0x20, 0x18, 0x20, 0x7F], // W
    [0x63, 0x14, 0x08, 0x14, 0x63], // X
    [0x07, 0x08, 0x70, 0x08, 0x07], // Y
    [0x61, 0x51, 0x49, 0x45, 0x43], // Z
];

fn glyph(c: u8) -> [u8; 5] {
    match c {
        b'0'..=b'9' => DIGITS[(c - b'0') as usize],
        b'A'..=b'Z' => ALPHA[(c - b'A') as usize],
        b'a'..=b'z' => ALPHA[(c - b'a') as usize], // fold lowercase onto uppercase
        b'%' => [0x23, 0x13, 0x08, 0x64, 0x62],
        b'(' => [0x00, 0x1C, 0x22, 0x41, 0x00],
        b')' => [0x00, 0x41, 0x22, 0x1C, 0x00],
        b'-' => [0x08, 0x08, 0x08, 0x08, 0x08],
        b'.' => [0x00, 0x60, 0x60, 0x00, 0x00],
        b'/' => [0x20, 0x10, 0x08, 0x04, 0x02],
        b':' => [0x00, 0x36, 0x36, 0x00, 0x00],
        b'>' => [0x00, 0x41, 0x22, 0x14, 0x08],
        b'=' => [0x14, 0x14, 0x14, 0x14, 0x14],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00], // space + anything unmapped
    }
}
