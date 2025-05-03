from sys import platform
import time
import gc
from machine import Pin, freq
from ir_rx.print_error import print_error
from ir_rx.nec import NEC_16  # Only import NEC_16

import network  # Import the network module for WiFi connectivity

# WiFi credentials
WIFI_SSID = "desk"
WIFI_PASSWORD = "testing123"

def connect():
    #Connect to WLAN
    wlan = network.WLAN(network.STA_IF)
    wlan.active(True)
    wlan.connect(WIFI_SSID, WIFI_PASSWORD)
    while wlan.isconnected() == False:
        print('Waiting for connection...')
        time.sleep(1)
    ip = wlan.ifconfig()[0]
    print(f'Connected on {ip}')


def cb(data, addr, ctrl):
    if data < 0:  # NEC protocol sends repeat codes.
        print("Repeat code.")
    else:
        print(f"Data 0x{data:02x} Addr 0x{addr:04x} Ctrl 0x{ctrl:02x}")

def recv():
    p = Pin(16, Pin.IN)  # Define the pin for IR receiver
    ir = NEC_16(p, cb)  # Instantiate NEC_16 receiver
    ir.error_function(print_error)  # Show debug information

    # code here
    try:
        while True:
            print("Running...")
            time.sleep(5)
            gc.collect()
    except KeyboardInterrupt:
        ir.close()

connect()

recv()