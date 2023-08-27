use std::{thread::sleep, time::Duration};

use embedded_svc::wifi::{ClientConfiguration, Configuration};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition, wifi::EspWifi};
use esp_idf_sys as _;

use log::*;

use reqwest;

use serde::Deserialize;

#[derive(Deserialize)]
struct DoorStatus {
    executed: u8,
    up: u8,
    amount: u8,
    over_ride: u8,
    over_ride_day: u16,
}

async fn main() {
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

            let response = match reqwest::get("https://httpbin.org/ip").await {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Response error | {}", e);
                }
            };

            let door_status = match response.json::<DoorStatus>().await {
                Ok(ds) => ds,
                Err(e) => {
                    error!("Deserialization error | {}", e);
                }
            };

            println!("Executed: {}", door_status.executed);

            // if executed == 0
            // if direction up
            // execute up
            // else
            //execute down

            // flip to executed
        } else {
            warn!("Waiting for connection...");
        }

        sleep(Duration::new(10, 0));
    }
}
