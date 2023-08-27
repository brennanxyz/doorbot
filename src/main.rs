use std::{thread::sleep, time::Duration};

use embedded_svc::wifi::{ClientConfiguration, Configuration};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::client::{Configuration as CConfiguration, EspHttpConnection},
    nvs::EspDefaultNvsPartition,
    wifi::EspWifi,
};

use esp_idf_sys as _;

use core::str;
use embedded_svc::{
    http::{client::Client, Status},
    io::Read,
};

use log::*;

// use ureq::{Agent, Error, MiddlewareNext, Request, Response, TlsConnector};
// use minreq;

use serde::Deserialize;

#[derive(Deserialize)]
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
            // hit API
            println!("Hit API");

            get("https://chicken.brennanharris.xyz");

            // match ureq::get("https://chicken.brennanharris.xyz").call() {
            //     Ok(resp) => {
            //         info!("succeeded");
            //     }
            //     Err(e) => {
            //         error!("failed | {}", e);
            //     }
            // }
        } else {
            warn!("Waiting for connection...");
        }

        sleep(Duration::new(10, 0));
    }
}

fn get(url: impl AsRef<str>) -> () {
    // 1. Create a new EspHttpClient. (Check documentation)
    let connection = match EspHttpConnection::new(&CConfiguration {
        use_global_ca_store: true,
        crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
        ..Default::default()
    }) {
        Ok(conn) => conn,
        Err(e) => {
            error!("GET ERR | {}", e);
            return ();
        }
    };
    let mut client = Client::wrap(connection);

    // 2. Open a GET request to `url`
    let request = match client.get(url.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            error!("REQ ERR | {}", e);
            return ();
        }
    };

    // 3. Submit write request and check the status code of the response.
    // Successful http status codes are in the 200..=299 range.
    let response = match request.submit() {
        Ok(resp) => resp,
        Err(e) => {
            error!("RES ERR | {}", e);
            return;
        }
    };
    let status = response.status();

    println!("Response code: {}\n", status);

    match status {
        200..=299 => {
            // 4. if the status is OK, read response data chunk by chunk into a buffer and print it until done
            let mut buf = [0_u8; 256];
            let mut reader = response;
            loop {
                if let Ok(size) = Read::read(&mut reader, &mut buf) {
                    if size == 0 {
                        break;
                    }
                    // 5. try converting the bytes into a Rust (UTF-8) string and print it
                    let response_text = match str::from_utf8(&buf[..size]) {
                        Ok(rt) => rt,
                        Err(e) => {
                            error!("RESP TEXT ERR | {}", e);
                            return;
                        }
                    };
                    println!("RESPONSE TEXT SUCCESS | {}", response_text);
                }
            }
        }
        _ => error!("Unexpected response code: {}", status),
    }

    ()
}
