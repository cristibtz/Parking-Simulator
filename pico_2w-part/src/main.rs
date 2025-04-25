#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_time::{Duration, Timer};
use embassy_net::tcp::TcpSocket;
use static_cell::StaticCell;
use cyw43::JoinOptions;
use embassy_rp::pwm::{Config as PwmConfig, Pwm};
use fixed::traits::ToFixed;
use {defmt_rtt as _, panic_probe as _};

use defmt::*;

mod irqs;

const SOCK: usize = 4;
static RESOURCES: StaticCell<StackResources<SOCK>> = StaticCell::<StackResources<SOCK>>::new();
const WIFI_NETWORK: &str = "desk";
const WIFI_PASSWORD: &str = "testing123";

#[embassy_executor::main]
async fn main(spawner: Spawner) {

    let peripherals = embassy_rp::init(Default::default());

    // Init WiFi driver
    let (net_device, mut control) = embassy_lab_utils::init_wifi!(&spawner, peripherals).await;

    // Default config for dynamic IP address
    let config = embassy_net::Config::dhcpv4(Default::default());

    // Init network stack
    let stack = embassy_lab_utils::init_network_stack(&spawner, net_device, &RESOURCES, config);

    //Connect to WiFi
    loop {
        match control.join(WIFI_NETWORK, JoinOptions::new_open()).await {
            Ok(_) => {
                info!("Successfully joined WiFi network: {}", WIFI_NETWORK);

                info!("Waiting for DHCP...");
                loop {
                    if stack.is_config_up() {
                        if let Some(ip_config) = stack.config_v4() {
                            let ip = ip_config.address.address(); 
                            info!("Assigned IP address: {}", ip);
                            break; 
                        }
                    }
                    Timer::after_millis(100).await;
                }
                break; 
            }
            Err(err) => {
                info!("Join failed with status={}", err.status);
                Timer::after(Duration::from_secs(1)).await; 
            }
        }
    }

    // Start TCP server

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

    socket.set_timeout(Some(Duration::from_secs(1000)));
    info!("Listening on TCP:6000...");
    if let Err(e) = socket.accept(6000).await {
        warn!("accept error: {:?}", e);
        return;
    }

    info!("Received connection from {:?}", socket.remote_endpoint());
    let mut buf = [0; 4096];

    //Config pwm and servo
    // Configure PWM for servo control
    let mut servo_config: PwmConfig = Default::default();

    // Set the calculated TOP value for 50 Hz PWM
    servo_config.top = 0xB71A; 

    // Set the clock divider to 64
    servo_config.divider = 64_i32.to_fixed(); // Clock divider = 64

    // Servo timing constants
    const PERIOD_US: usize = 20_000; // 20 ms period for 50 Hz
    const MIN_PULSE_US: usize = 500; // 0.5 ms pulse for 0 degrees
    const MAX_PULSE_US: usize = 2500; // 2.5 ms pulse for 180 degrees

    // Calculate the PWM compare values for minimum and maximum pulse widths
    let min_pulse = ((MIN_PULSE_US * servo_config.top as usize) / PERIOD_US) as u16;
    let max_pulse = ((MAX_PULSE_US * servo_config.top as usize) / PERIOD_US) as u16;

    // Initialize PWM for servo control
    let mut servo = Pwm::new_output_a(
        peripherals.PWM_SLICE1, 
        peripherals.PIN_2, 
        servo_config.clone()
    );

    // State variable to track whether the barrier is open or closed
    let mut is_open = false;

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
    
        // Parse the received data as a command
        if let Ok(command) = core::str::from_utf8(&buf[..n]) {
            match command.trim() {
                "100" => {
                    if !is_open {
                        // Open the barrier
                        servo_config.compare_a = min_pulse * 2; // Open position
                        servo.set_config(&servo_config);
                        info!("Barrier opened");
                        is_open = true;
                    } else {
                        info!("Barrier is already open");
                    }
                }
                "90" => {
                    if is_open {
                        // Close the barrier manually
                        servo_config.compare_a = max_pulse; // Closed position
                        servo.set_config(&servo_config);
                        info!("Barrier closed manually");
                        is_open = false;
                    } else {
                        info!("Barrier is already closed");
                    }
                }
                _ => {
                    warn!("Unknown command received: {}", command);
                }
            }
        }
    
        // Add a small delay to prevent busy looping
        Timer::after(Duration::from_millis(100)).await;
    }
}