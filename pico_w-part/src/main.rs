#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};
use defmt::*;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize peripherals and USB driver.
    let rp_peripherals = embassy_rp::init(Default::default());
    
    Timer::after_millis(1000).await;
    info!("Hello, world!");

    loop {
        Timer::after_millis(10).await;
    }
}