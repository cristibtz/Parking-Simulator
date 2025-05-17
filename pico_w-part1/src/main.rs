#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use core::fmt::Write as FmtWrite;
use core::str::FromStr;

use embassy_time::{Timer, Duration};
use cyw43::JoinOptions;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::StackResources;
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
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<embassy_rp::peripherals::I2C0>;
});

mod irqs;

const SOCK: usize = 8;
static RESOURCES: StaticCell<StackResources<SOCK>> = StaticCell::<StackResources<SOCK>>::new();
const WIFI_NETWORK: &str = "desk";
const WIFI_PASSWORD: &str = "testing123";

#[derive(Debug, PartialEq)]
enum SensorState {
    Occupied,
    NotOccupied,
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Starting...");

    let peripherals = embassy_rp::init(Default::default());

    // Init WiFi driver
    let (net_device, mut control) = embassy_lab_utils::init_wifi!(&spawner, peripherals).await;

    // Default config for dynamic IP address
    let config = embassy_net::Config::dhcpv4(Default::default());

    // Init network stack
    let stack = embassy_lab_utils::init_network_stack(&spawner, net_device, &RESOURCES, config);
    
    // Configure I2C for SSD1306 OLED Display
    let i2c = i2c::I2c::new_async(
        peripherals.I2C0,
        peripherals.PIN_5, // SCL
        peripherals.PIN_4, // SDA
        Irqs,
        I2cConfig::default(),
    );

    // Initialize SSD1306 OLED Display
    let interface = I2CDisplayInterface::new(i2c);
    let mut display: Ssd1306<_, _, _> = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display.init().unwrap();
    display.clear(BinaryColor::Off).unwrap();

    // Parking lot state
    const TOTAL_SPACES: u64 = 4;
    let mut free_spaces = TOTAL_SPACES;

    // Array to track the state of sensors 1, 2, 3, and 4
    let mut sensor_states = [const { SensorState::NotOccupied }; 4];
    
    // Connect to WiFi
    loop {
        match control
            .join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
            .await
        {
            Ok(_) => {
                info!("Successfully joined WiFi network: {}", WIFI_NETWORK);

                // Wait until the network stack is configured
                loop {
                    if stack.is_config_up() {
                        if let Some(ip_config) = stack.config_v4() {
                            let ip = ip_config.address.address();
                            info!("Assigned IP address: {}", ip);

                            // Display the IP address on the OLED
                            display.clear(BinaryColor::Off).unwrap();
                            let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
                            Text::new("WiFi Connected!", Point::new(0, 8), text_style)
                                .draw(&mut display)
                                .unwrap();
                            Text::new("IP Address:", Point::new(0, 16), text_style)
                                .draw(&mut display)
                                .unwrap();
                            let mut ip_buffer = heapless::String::<64>::new();
                            FmtWrite::write_fmt(&mut ip_buffer, format_args!("{}", ip)).unwrap();
                            Text::new(&ip_buffer, Point::new(0, 32), text_style)
                                .draw(&mut display)
                                .unwrap();
                            display.flush().unwrap();

                            break;
                        }
                    }
                    Timer::after(Duration::from_millis(100)).await;
                }
                break;
            }
            Err(err) => {
                info!("Join failed with status={}", err.status);
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    }

    // Listen for incoming TCP connections on port 6000
    loop {
        info!("Listening on TCP:6000...");
        let mut rx_buffer = [0; 4096];
        let mut tx_buffer = [0; 4096];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    
        if let Err(e) = socket.accept(6000).await {
            warn!("accept error: {:?}", e);
            continue;
        }
    
        info!("Received connection from {:?}", socket.remote_endpoint());
        let mut buf = [0; 4096];
    
        loop {
            // Read data from the socket
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    warn!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {:?}", e);
                    break;
                }
            };
    
            if let Ok(data) = core::str::from_utf8(&buf[..n]) {
                info!("Received data: {}", data);
    
                if let Some((sensor_no, new_state)) = parse_sensor_data(data) {
                    if sensor_no >= 1 && sensor_no <= 4 {
                        let sensor_index = (sensor_no - 1) as usize;
    
                        // Update parking lot state only if the sensor state changes
                        if sensor_states[sensor_index] != new_state {
                            match new_state {
                                SensorState::Occupied => {
                                    if free_spaces > 0 {
                                        free_spaces -= 1;
                                    }
                                }
                                SensorState::NotOccupied => {
                                    if free_spaces < TOTAL_SPACES {
                                        free_spaces += 1;
                                    }
                                }
                            }
    
                            // Update the sensor state
                            sensor_states[sensor_index] = new_state;
    
                            // Update the OLED display
                            display.clear(BinaryColor::Off).unwrap();
                            let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
                            let mut parking_status = heapless::String::<64>::new();
                            FmtWrite::write_fmt(
                                &mut parking_status,
                                format_args!("Free spaces: {}/{}", free_spaces, TOTAL_SPACES)
                            )
                            .unwrap();
                            Text::new(&parking_status, Point::new(0, 8), text_style)
                                .draw(&mut display)
                                .unwrap();
                            display.flush().unwrap();
    
                            // Close the socket after processing the state change
                            socket.close();
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Parses the received data in the format `Sensor x: Occupied` or `Sensor x: Not Occupied`.
fn parse_sensor_data(data: &str) -> Option<(u64, SensorState)> {
    let data = data.trim();

    if let Some((sensor_part, state_part)) = data.split_once(":") {
        let sensor_part = sensor_part.trim();
        let state_part = state_part.trim();

        if sensor_part.starts_with("Sensor") {
            if let Some(sensor_no_str) = sensor_part.strip_prefix("Sensor ") {
                if let Ok(sensor_no) = u64::from_str(sensor_no_str) {
                    if state_part == "Occupied" {
                        return Some((sensor_no, SensorState::Occupied));
                    } else if state_part == "Not Occupied" {
                        return Some((sensor_no, SensorState::NotOccupied));
                    }
                }
            }
        }
    }

    None
}