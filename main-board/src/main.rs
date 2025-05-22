#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::{IpAddress, IpEndpoint, Stack, StackResources};
use embassy_time::{Duration, Timer};
use embassy_net::tcp::TcpSocket;
use static_cell::StaticCell;
use cyw43::JoinOptions;
use embassy_rp::{gpio::{AnyPin, Input, Level, Output, Pin, Pull}, pwm::{Config as PwmConfig, Pwm}};
use fixed::traits::ToFixed;
use {defmt_rtt as _, panic_probe as _};
use heapless::String; 

use defmt::*;

mod irqs;

const SOCK: usize = 8;
static RESOURCES: StaticCell<StackResources<SOCK>> = StaticCell::<StackResources<SOCK>>::new();
const WIFI_NETWORK: &str = "desk";
const WIFI_PASSWORD: &str = "testing123";

#[embassy_executor::task(pool_size = 4)]
async fn sensor_task(pin: AnyPin, mut led_green: Output<'static>, mut led_red: Output<'static>, stack: Stack<'static>, sensor_no: u64) {
    let sensor = Input::new(pin, Pull::Up);

    loop {
        // Check the sensor state
        let mut state: String<128> = String::new();
        if !sensor.is_high() {
            let _ = core::fmt::write(&mut state, format_args!("Sensor {}: Occupied", sensor_no));
            // Turn on the red LED
            led_red.set_high();
            led_green.set_low();
        } else {
            let _ = core::fmt::write(&mut state, format_args!("Sensor {}: Not Occupied", sensor_no));
            // Turn off the red LED
            led_red.set_low();
            led_green.set_high();
        }

        // Create a new TcpSocket for each connection attempt
        let mut tx_buffer = [0; 128];
        let mut rx_buffer = [0; 128];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        // Connect to the TCP server
        match socket.connect(IpEndpoint::new(IpAddress::v4(192, 168, 23, 41), 6000)).await {
            Ok(_) => {
                info!("Connected to server");

                // Send the sensor state
                let buffer = state.as_bytes();
                if let Err(e) = socket.write(buffer).await {
                    warn!("write error: {:?}", e);
                } else {
                    info!("Sent state: {}", state.as_str());
                }

                // Close the socket
                socket.close();
            }
            Err(e) => {
                warn!("connect error: {:?}", e);
            }
        }

        // Wait before checking the sensor state again
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {

    let peripherals = embassy_rp::init(Default::default());

    // Barrier LED pins
    let mut barrier_led_open = Output::new(peripherals.PIN_16, Level::Low);
    let mut barrier_led_closed = Output::new(peripherals.PIN_17, Level::High);

    // Init WiFi driver
    let (net_device, mut control) = embassy_lab_utils::init_wifi!(&spawner, peripherals).await;

    // Default config for dynamic IP address
    let config = embassy_net::Config::dhcpv4(Default::default());

    // Init network stack
    let stack = embassy_lab_utils::init_network_stack(&spawner, net_device, &RESOURCES, config);

    //Connect to WiFi
    loop {
        match control.join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes())).await {
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
                Timer::after(Duration::from_secs(3)).await; 
            }
        }
    }

    //Start the sensor task
    let sensor_no1:u64 = 1;
    let pin_27_clone = Output::new(peripherals.PIN_27, Level::Low);
    let pin_26_clone = Output::new(peripherals.PIN_26, Level::Low);
    let pin_14_clone = peripherals.PIN_14.degrade();
    spawner.spawn(sensor_task(pin_14_clone, pin_26_clone, pin_27_clone, stack, sensor_no1)).unwrap(); 

    let sensor_no2:u64 = 2;
    let pin_3_clone = Output::new(peripherals.PIN_3, Level::Low);
    let pin_4_clone = Output::new(peripherals.PIN_4, Level::Low);
    let pin_15_clone = peripherals.PIN_15.degrade();
    spawner.spawn(sensor_task(pin_15_clone, pin_3_clone, pin_4_clone, stack, sensor_no2)).unwrap();

    let sensor_no3:u64 = 3;
    let pin_6_clone = Output::new(peripherals.PIN_6, Level::Low);
    let pin_7_clone = Output::new(peripherals.PIN_7, Level::Low);
    let pin_18_clone = peripherals.PIN_18.degrade();
    spawner.spawn(sensor_task(pin_18_clone, pin_6_clone, pin_7_clone, stack, sensor_no3)).unwrap();

    let sensor_no4:u64 = 4;
    let pin_8_clone = Output::new(peripherals.PIN_8, Level::Low);
    let pin_9_clone = Output::new(peripherals.PIN_9, Level::Low);
    let pin_19_clone = peripherals.PIN_19.degrade();
    spawner.spawn(sensor_task(pin_19_clone, pin_8_clone, pin_9_clone, stack, sensor_no4)).unwrap();

    // Start TCP server

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

    socket.set_timeout(Some(Duration::from_secs(1000)));

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

    loop {
        // Accept a new connection
        info!("Listening on TCP:6000...");
        let mut rx_buffer = [0; 4096]; // Move buffer initialization here
        let mut tx_buffer = [0; 4096]; // Move buffer initialization here
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    
        if let Err(e) = socket.accept(6000).await {
            warn!("accept error: {:?}", e);
            continue; // Continue to the next iteration to accept a new connection
        }
    
        info!("Received connection from {:?}", socket.remote_endpoint());
        let mut buf = [0; 4096];
    
        // State variables
        let mut is_open = false; // Tracks whether the barrier is open
        let mut is_locked = false; // Tracks whether the barrier is locked
    
        // Ensure the closed LED is red by default
        barrier_led_closed.set_high(); // Red LED ON
        barrier_led_open.set_low();    // Green LED OFF
    
        loop {
            // Read data from the socket
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    warn!("read EOF");
                    break; // Exit the inner loop to accept a new connection
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {:?}", e);
                    break; // Exit the inner loop to accept a new connection
                }
            };
    
            // Parse the received data as a command
            if let Ok(command) = core::str::from_utf8(&buf[..n]) {
                match command.trim() {
                    "100" => {
                        if is_locked {
                            info!("Barrier is locked. Cannot open.");
                        } else if !is_open {
                            // Open the barrier
                            servo_config.compare_a = min_pulse * 2; // Open position
                            servo.set_config(&servo_config);
                            info!("Barrier opened");
                            is_open = true;
    
                            // Update LEDs: Green ON, Red OFF
                            barrier_led_open.set_high();  // Green LED ON
                            barrier_led_closed.set_low(); // Red LED OFF
    
                            // Automatically close the barrier after 5 seconds
                            Timer::after(Duration::from_secs(5)).await;
                            servo_config.compare_a = max_pulse; // Closed position
                            servo.set_config(&servo_config);
                            info!("Barrier closed automatically");
                            is_open = false;
    
                            // Update LEDs: Red ON, Green OFF
                            barrier_led_open.set_low();   // Green LED OFF
                            barrier_led_closed.set_high(); // Red LED ON
                        } else {
                            info!("Barrier is already open");
                        }
                    }
                    "90" => {
                        if is_locked {
                            // Unlock the barrier
                            is_locked = false;
                            info!("Barrier unlocked");
                        } else {
                            // Lock the barrier
                            is_locked = true;
                            info!("Barrier locked");
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
}