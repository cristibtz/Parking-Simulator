#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use core::fmt::Write;

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::i2c::{self, Config as I2cConfig};
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::text::Text;
use ssd1306::prelude::*;
use ssd1306::size::DisplaySize128x64;
use ssd1306::I2CDisplayInterface;
use ssd1306::Ssd1306;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::Drawable;
use embedded_graphics::prelude::Point;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<embassy_rp::peripherals::I2C0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("Starting...");

    let peripherals = embassy_rp::init(Default::default());

    // Configure I2C for SSD1306 OLED Display
    let i2c = i2c::I2c::new_async(
        peripherals.I2C0,
        peripherals.PIN_5, //scl 
        peripherals.PIN_4, //sda
        Irqs,
        I2cConfig::default(),
    );

    // Initialize SSD1306 OLED Display
    let interface = I2CDisplayInterface::new(i2c);
    
    let mut display: Ssd1306<_, _, _> = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
        display.init().unwrap();
    
    display.clear(BinaryColor::Off).unwrap();
    // Draw "Hello, World!" on the display
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new("Hello, World!", Point::new(0, 10), text_style)
        .draw(&mut display)
        .unwrap();

    display.flush().unwrap();

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}