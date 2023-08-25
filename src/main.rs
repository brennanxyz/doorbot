use std::{thread::sleep, time::Duration};

use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition, wifi::EspWifi};
use esp_idf_sys as _;
use log::*;

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

    match wifi_driver.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: "".into(), // TODO: replace with env vars
        password: "".into(),
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
        println!(
            "IP info: {:?}",
            wifi_driver.sta_netif().get_ip_info().unwrap()
        );
        sleep(Duration::new(15, 0));
    }
}
