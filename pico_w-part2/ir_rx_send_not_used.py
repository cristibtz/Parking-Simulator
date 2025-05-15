from sys import platform
import time
import gc
from machine import Pin, freq
from ir_rx.print_error import print_error
from ir_rx.nec import NEC_16  # Only import NEC_16

import network  # Import the network module for WiFi connectivity
import socket  # Import the socket module for network communication

# WiFi credentials
WIFI_SSID = "desk"
WIFI_PASSWORD = "testing123"
HOST = "192.168.23.155"

def connect():
    # Connect to WLAN
    wlan = network.WLAN(network.STA_IF)
    wlan.active(True)
    wlan.connect(WIFI_SSID, WIFI_PASSWORD)
    while wlan.isconnected() == False:
        print('Waiting for connection...')
        time.sleep(1)
    ip = wlan.ifconfig()[0]
    print(f'Connected on {ip}')
    return True

def reconnect_to_server():
    global sock, connected
    try:
        print("Reconnecting to server...")
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((HOST, 6000))
        connected = True
        print("Reconnected to server")
    except Exception as e:
        print(f"Failed to reconnect to server: {e}")
        connected = False

def send_data(data, close_socket=False):
    global connected
    if close_socket:
        sock.close()
        connected = False
        print("Socket closed")
        return

    if not connected:
        reconnect_to_server()
        if not connected:
            print("Unable to reconnect to server. Data not sent.")
            return

    try:
        sock.sendall(data)
        print("Data sent successfully")
    except Exception as e:
        print(f"Error sending data: {e}")
        connected = False  # Mark as disconnected if sending fails

def cb(data, addr, ctrl):
    if data < 0:
        print("Repeat code.")
    else:
        if data == 0x45:
            print("Opening")
            send_data(b"100", False)
        elif data == 0x46:
            print("Locking/Unlocking")
            send_data(b"90", False)
        elif data == 0x47:
            print("Closing")
            send_data(b"0", True)

        print(f"Data 0x{data:02x} Addr 0x{addr:04x} Ctrl 0x{ctrl:02x}")

def recv():
    p = Pin(16, Pin.IN)  # Define the pin for IR receiver
    ir = NEC_16(p, cb)  # Instantiate NEC_16 receiver
    ir.error_function(print_error)  # Show debug information

    try:
        while True:
            print("Running...")
            time.sleep(5)
            gc.collect()
    except KeyboardInterrupt:
        ir.close()

connected = connect()

if connected:
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((HOST, 6000))
        connected = True
        print("Connected to server")
    except Exception as e:
        print(f"Failed to connect to server: {e}")
        connected = False

recv()