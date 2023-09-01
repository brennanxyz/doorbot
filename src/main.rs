use std::{thread::sleep, time::Duration};

use embedded_svc::{
    http::{client::Client, Method},
    io::{Read, Write},
    wifi::{ClientConfiguration, Configuration},
};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::client::{Configuration as CConfiguration, EspHttpConnection},
    nvs::EspDefaultNvsPartition,
    wifi::EspWifi,
};

use esp_idf_sys as _;

use core::str;

use log::*;

// use ureq::{Agent, Error, MiddlewareNext, Request, Response, TlsConnector};
// use minreq;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct DoorStatus {
    executed: u8,
    up: u8,
    amount: u8,
    over_ride: u8,
    over_ride_day: u16,
}

fn main() {
    esp_idf_sys::link_patches(); // don't remove
    esp_idf_svc::log::EspLogger::initialize_default();
    info!("Patches linked. Main loop entered.");

    let peripherals = match Peripherals::take() {
        Some(p) => {
            info!("Peripherals taken.");
            p
        }
        None => {
            error!("Peripheral taking FAILED");
            panic!();
        }
    };

    let sys_loop = match EspSystemEventLoop::take() {
        Ok(sl) => {
            info!("Event loop taken.");
            sl
        }
        Err(e) => {
            error!("Event loop taking FAILED | {}", e);
            panic!();
        }
    };

    let nvs = match EspDefaultNvsPartition::take() {
        Ok(part) => {
            info!("Partition taken.");
            part
        }
        Err(e) => {
            error!("Partition taking FAILED | {}", e);
            panic!();
        }
    };

    let mut wifi_driver = match EspWifi::new(peripherals.modem, sys_loop, Some(nvs)) {
        Ok(wfd) => {
            info!("WiFi driver made.");
            wfd
        }
        Err(e) => {
            error!("WiFi driver FAILED | {}", e);
            panic!();
        }
    };

    let strings = std::include_str!("../.env");
    let string_array: Vec<&str> = strings.split("\n").collect();
    let wifi_ssid = string_array[0];
    let wifi_password = string_array[1];
    let api_secret = string_array[2];
    let get_address = string_array[3];
    let put_address = string_array[4];

    info!("Connecting to {}...", wifi_ssid);

    match wifi_driver.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: wifi_ssid.into(),
        password: wifi_password.into(),
        ..Default::default()
    })) {
        Ok(_) => {
            info!("WiFi driver configured.");
        }
        Err(e) => {
            error!("WiFi driver configuration FAILED | {}", e);
            panic!();
        }
    };

    match wifi_driver.start() {
        Ok(_) => {
            info!("WiFi driver started.");
        }
        Err(e) => {
            error!("WiFi driver start FAILED | {}", e);
            panic!();
        }
    };

    match wifi_driver.connect() {
        Ok(_) => {
            info!("WiFi driver initiated connection.");
        }
        Err(e) => {
            error!("WiFi driver connection initiation FAILED | {}", e);
            panic!();
        }
    };

    while !wifi_driver.is_connected().unwrap() {
        let config = wifi_driver.get_configuration().unwrap();
        warn!("Waiting for station {:?}...", config);
    }

    info!("Connected.");

    loop {
        let info = match wifi_driver.sta_netif().get_ip_info() {
            Ok(i) => i.ip.to_string(),
            Err(e) => {
                error!("WiFi check error | {}", e);
                "0.0.0.0".to_string()
            }
        };

        if info != *"0.0.0.0" {
            let status_string = get(get_address, Some(api_secret));

            match serde_json::from_str::<DoorStatus>(&status_string) {
                Ok(mut ds) => {

                    if ds.executed == 0 {
                        move_door(ds.up, ds.amount);
                    }


                    match serde_json::to_string::<DoorStatus>(&ds) {
                        Ok(ds_string) => {

                            info!("DoorStatus | {}", ds_string);

                            let put_response = put(
                                put_address,
                                api_secret,
                                &ds_string.as_bytes(),
                                ds_string.len(),
                            );

                            info!("PUT SUCCESS | {}", put_response);

                        }
                        Err(e) => {
                            error!("DoorStatus parse error | {}", e);
                        }
                    };
                }
                Err(e) => {
                    error!("DoorStatus parse error | {}", e);
                }
            }
        } else {
            warn!("Waiting for connection...");
        }

        sleep(Duration::new(10, 0));
    }
}

fn get(url: impl AsRef<str>, key: Option<&str>) -> String {
    let connection = match EspHttpConnection::new(&CConfiguration {
        use_global_ca_store: true,
        crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
        ..Default::default()
    }) {
        Ok(conn) => conn,
        Err(e) => {
            error!("GET ERR | {}", e);
            return "".to_string();
        }
    };
    let mut client = Client::wrap(connection);
    let keyv = key.unwrap_or("");
    let binding = [("X-Access-Key", keyv)];
    let request = match client.request(Method::Get, url.as_ref(), &binding) {
        Ok(r) => r,
        Err(e) => {
            error!("REQ ERR (with key) | {}", e);
            return "".to_string();
        }
    };

    let response = match request.submit() {
        Ok(resp) => resp,
        Err(e) => {
            error!("RES ERR | {}", e);
            return "".to_string();
        }
    };
    let status = response.status();

    println!("Response code: {}\n", status);

    match status {
        200..=299 => {
            let mut buf = [0_u8; 256];
            let mut reader = response;
            let mut resp_text = "".to_string();
            loop {
                if let Ok(size) = Read::read(&mut reader, &mut buf) {
                    if size == 0 {
                        break;
                    }
                    let response_text = match str::from_utf8(&buf[..size]) {
                        Ok(rt) => rt,
                        Err(e) => {
                            error!("RESP TEXT ERR | {}", e);
                            return "".to_string();
                        }
                    };
                    resp_text += response_text;
                }
            }
            resp_text
        }
        _ => {
            error!("Unexpected response code: {}", status);
            "".to_string()
        }
    }
}

fn put(url: impl AsRef<str>, key: &str, payload: &[u8], str_length: usize) -> String {
    let connection = match EspHttpConnection::new(&CConfiguration {
        use_global_ca_store: true,
        crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
        ..Default::default()
    }) {
        Ok(conn) => conn,
        Err(e) => {
            error!("CONN ERR | {}", e);
            return "".to_string();
        }
    };
    let mut client = Client::wrap(connection);
    let binding = [
        ("X-Access-Key", key),
        ("Content-Type", "application/json"),
        ("Content-Length", &str_length.to_string()),
    ];

    let mut request = match client.put(url.as_ref(), &binding) {
        Ok(r) => r,
        Err(e) => {
            error!("REQ ERR (with key) | {}", e);
            return "".to_string();
        }
    };

    // write data
    match request.write(payload) {
        Ok(_) => {
            let response = match request.submit() {
                Ok(resp) => resp,
                Err(e) => {
                    error!("RES ERR | {}", e);
                    return "".to_string();
                }
            };

            let status = response.status();

            println!("Response code: {}\n", status);
            println!("Response message: {:?}\n", response.status_message());

            match status {
                200..=299 => {
                    let mut buf = [0_u8; 256];
                    let mut reader = response;
                    let mut resp_text = "".to_string();
                    loop {
                        if let Ok(size) = Read::read(&mut reader, &mut buf) {
                            if size == 0 {
                                break;
                            }
                            let response_text = match str::from_utf8(&buf[..size]) {
                                Ok(rt) => rt,
                                Err(e) => {
                                    error!("RESP TEXT ERR | {}", e);
                                    return "".to_string();
                                }
                            };
                            resp_text += response_text;
                        }
                    }
                    resp_text
                }
                _ => {
                    error!("Unexpected response code: {}", status);
                    "".to_string()
                }
            }
        }
        Err(e) => {
            error!("Payload not written | {}", e);
            "".to_string()
        }
    }
}

fn move_door(up: u8, amount: u8) {
    info!("Moving door {} by {}", if up == 1 { "up" } else { "down" }, amount);
    // find a PWM library
    // follow this: https://lastminuteengineers.com/l298n-dc-stepper-driver-arduino-tutorial/
    info!("Door stopped");
}