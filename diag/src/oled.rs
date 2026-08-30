//! SSD1306 128x32 status display via the `ssd1306` crate over our embedded-hal shim
//! ([`crate::hal::EusciI2c`]). First consumer of the board HAL; also the compile-proof that the
//! embedded-graphics stack builds for msp430.

use crate::hal::EusciI2c;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use msp430fr2476::Peripherals;
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

/// How far the render got — reported over UART so we know whether the panel actually took the
/// data, independent of anyone reading the glass. `init`/`flush` are the two I2C touch-points
/// (drawing is into a RAM buffer and can't fail).
pub enum Status {
    Ok,
    InitFail,
    FlushFail,
}

/// Render the POST verdict. `init` and `flush` are the only fallible (bus-touching) steps.
pub fn show_status(p: &Peripherals, ok: bool, present: u16, total: u16) -> Status {
    let i2c = EusciI2c::new(p);
    let interface = I2CDisplayInterface::new(i2c); // defaults to 0x3C
    let mut display = Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    if display.init().is_err() {
        return Status::InitFail;
    }
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    // "found N/M" without alloc — counts are single-digit on any real board.
    let mut l1 = *b"found 0/0";
    l1[6] = b'0' + present.min(9) as u8;
    l1[8] = b'0' + total.min(9) as u8;
    let line1 = core::str::from_utf8(&l1).unwrap_or("found");
    let line2 = if ok { "OK" } else { "FAULT" };

    let _ = display.clear(BinaryColor::Off);
    let _ = Text::with_baseline(line1, Point::new(0, 0), style, Baseline::Top).draw(&mut display);
    let _ = Text::with_baseline(line2, Point::new(0, 16), style, Baseline::Top).draw(&mut display);
    if display.flush().is_err() {
        return Status::FlushFail;
    }
    Status::Ok
}
