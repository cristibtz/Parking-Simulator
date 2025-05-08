#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::Peripherals;
use embassy_time::{Duration, Instant, Timer};
use {defmt_rtt as _, panic_probe as _};

const MAX_PULSES: usize = 200;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut ir_sensor = Input::new(p.PIN_15, Pull::None);

    info!("Press a button on the remote...");

    loop {
        // Wait for falling edge to begin
        ir_sensor.wait_for_falling_edge().await;

        let mut pulses: [u32; MAX_PULSES] = [0; MAX_PULSES];
        let mut count = 0;
        let mut start_time = Instant::now();

        while count < MAX_PULSES {
            // Wait for LOW pulse
            while ir_sensor.is_low() {}
            let now = Instant::now();
            pulses[count] = now.duration_since(start_time).as_micros() as u32;
            count += 1;
            if count >= MAX_PULSES { break; }
            start_time = now;

            // Wait for HIGH pulse
            while ir_sensor.is_high() {}
            let now = Instant::now();
            pulses[count] = now.duration_since(start_time).as_micros() as u32;
            count += 1;
            if count >= MAX_PULSES { break; }
            start_time = now;
        }

        match decode_nec(&pulses[..count]) {
            Some((addr, cmd)) => info!("✅ NEC Command: 0x{:02X} (Address: 0x{:02X})", cmd, addr),
            None => warn!("❌ Invalid NEC signal"),
        }

        Timer::after(Duration::from_millis(1000)).await;
    }
}

fn decode_nec(pulses: &[u32]) -> Option<(u8, u8)> {
    if pulses.len() < 66 {
        return None;
    }

    // Check NEC start pulse: ~9ms LOW, ~4.5ms HIGH
    if !(8500..9500).contains(&pulses[0]) || !(4000..5000).contains(&pulses[1]) {
        return None;
    }

    let mut bits: u32 = 0;

    // NEC data bits start at index 2 (after 9ms + 4.5ms header)
    for i in 0..32 {
        let low = pulses[2 + i * 2];     // Should be ~562us
        let high = pulses[2 + i * 2 + 1]; // 562us = 0, ~1.7ms = 1

        if !(400..700).contains(&low) {
            return None;
        }

        let bit = if (1300..1900).contains(&high) { 1 } else if (400..700).contains(&high) { 0 } else {
            return None;
        };

        bits |= (bit as u32) << i;
    }

    let addr = (bits & 0xFF) as u8;
    let addr_inv = ((bits >> 8) & 0xFF) as u8;
    let cmd = ((bits >> 16) & 0xFF) as u8;
    let cmd_inv = ((bits >> 24) & 0xFF) as u8;

    // Validate inverse bytes
    if addr ^ addr_inv == 0xFF && cmd ^ cmd_inv == 0xFF {
        Some((addr, cmd))
    } else {
        None
    }
}
