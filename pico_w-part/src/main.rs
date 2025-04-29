#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use core::str::from_utf8;

use cyw43::JoinOptions;
use defmt::*;
use embassy_net::StackResources;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod irqs;

const SOCK: usize = 4;
static RESOURCES: StaticCell<StackResources<SOCK>> = StaticCell::<StackResources<SOCK>>::new();
const WIFI_NETWORK: &str = "desk";
const WIFI_PASSWORD: &str = "testing123";

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello World!");

    let peripherals = embassy_rp::init(Default::default());


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

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut buf = [0; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        control.gpio_set(0, false).await;
        info!("Listening on TCP:1234...");
        if let Err(e) = socket.accept(1234).await {
            warn!("accept error: {:?}", e);
            continue;
        }

        info!("Received connection from {:?}", socket.remote_endpoint());
        control.gpio_set(0, true).await;

        loop {
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

            info!("rxd {}", from_utf8(&buf[..n]).unwrap());

            match socket.write_all(&buf[..n]).await {
                Ok(()) => {}
                Err(e) => {
                    warn!("write error: {:?}", e);
                    break;
                }
            };
        }
    }
}