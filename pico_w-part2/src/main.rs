#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::Peripherals;
use embassy_time::{Duration, Instant, Timer};
use embassy_net::StackResources;
use embassy_net::tcp::TcpSocket;
use cyw43::JoinOptions;
use embassy_net::IpAddress;
use embassy_net::IpEndpoint;
use static_cell::StaticCell;
use heapless::String;
use embedded_io_async::Write;
use fixed::traits::ToFixed;

use {defmt_rtt as _, panic_probe as _};

mod irqs;

const SOCK: usize = 4;
static RESOURCES: StaticCell<StackResources<SOCK>> = StaticCell::<StackResources<SOCK>>::new();
const WIFI_NETWORK: &str = "desk";
const WIFI_PASSWORD: &str = "testing123";

const MAX_PULSES: usize = 70;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let peripherals = embassy_rp::init(Default::default());
    let mut ir_sensor = Input::new(peripherals.PIN_15, Pull::None);

    // Init WiFi driver
    let (net_device, mut control) = embassy_lab_utils::init_wifi!(&spawner, peripherals).await;

    // Default config for dynamic IP address
    let config = embassy_net::Config::dhcpv4(Default::default());

    // Init network stack
    let stack = embassy_lab_utils::init_network_stack(&spawner, net_device, &RESOURCES, config);

    loop {
        match control
            .join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
            .await
        {
            Ok(_) => {
                // Successfully joined the WiFi network
                info!("Successfully joined WiFi network: {}", WIFI_NETWORK);

                // Wait until the network stack is configured
                loop {
                    if stack.is_config_up() {
                        if let Some(ip_config) = stack.config_v4() {
                            info!(
                                "Assigned IP address: {}",
                                ip_config.address.address()
                            );
                            break;
                        }
                    }
                    Timer::after(Duration::from_millis(100)).await;
                }
                break;
            }
            Err(err) => {
                info!("join failed with status={}", err.status);
            }
        }
    }

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

        let mut tx_buffer = [0; 128];
        let mut rx_buffer = [0; 128];

        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(5)));

        match decode_nec(&pulses[..count]) {
            Some((addr, cmd)) => {
                info!("✅ NEC Command: 0x{:02X} (Address: 0x{:02X})", cmd, addr);
        
                // Determine the data to send based on the command
                let data_to_send = if cmd == 0x45 {
                    "100"
                } else if cmd == 0x46 {
                    "90"
                } else {
                    warn!("Unknown command: 0x{:02X}", cmd);
                    continue; // Skip sending for unknown commands
                };
        
                // Connect to the TCP server
                if let Err(e) = socket.connect(IpEndpoint::new(IpAddress::v4(192, 168, 23, 155), 6000)).await {
                    warn!("Failed to connect to server: {:?}", e);
                    continue;
                }
        
                // Send the data as a single byte
                if let Err(e) = socket.write_all(data_to_send.as_bytes()).await {
                    warn!("Failed to send data: {:?}", e);
                } else {
                    info!("Sent data: {}", data_to_send);
                }
        
                // Close the socket
                socket.close();
            }
            None => warn!("❌ Invalid NEC signal"),
        }

        socket.close();

        Timer::after(Duration::from_millis(300)).await;
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
