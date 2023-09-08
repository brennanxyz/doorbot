use std::{thread::sleep, time::Duration};

use embedded_svc::{
    http::{client::Client, Method},
    io::Read,
    wifi::{ClientConfiguration, Configuration},
};

use esp_idf_hal::{
    gpio::{AnyOutputPin, OutputPin, PinDriver},
    peripherals::Peripherals,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::client::{Configuration as CConfiguration, EspHttpConnection},
    nvs::EspDefaultNvsPartition,
    wifi::EspWifi,
};

use esp_idf_sys as _;

use core::str;

use log::*;

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
    info!("Patches linked. Main fn entered.");

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

    let led = peripherals.pins.gpio2.downgrade_output();
    let mut led_driver = match PinDriver::output(led) {
        Ok(ld) => {
            info!("LED driver made.");
            ld
        }
        Err(e) => {
            error!("LED driver FAILED | {}", e);
            panic!();
        }
    };

    let motor_lead_1 = peripherals.pins.gpio16.downgrade_output();
    let mut motor_lead_1_driver = match PinDriver::output(motor_lead_1) {
        Ok(ml1d) => {
            info!("MOTOR LEAD 1 driver made.");
            ml1d
        }
        Err(e) => {
            error!("MOTOR LEAD 1 driver FAILED | {}", e);
            panic!();
        }
    };

    let motor_lead_2 = peripherals.pins.gpio17.downgrade_output();
    let mut motor_lead_2_driver = match PinDriver::output(motor_lead_2) {
        Ok(ml2d) => {
            info!("MOTOR LEAD 2 driver made.");
            ml2d
        }
        Err(e) => {
            error!("MOTOR LEAD 2 driver FAILED | {}", e);
            panic!();
        }
    };

    let mut try_put: bool;

    let mut ct = 0;

    flash_pattern(". . .  ", &mut led_driver);

    loop {
        flash_pattern("..  ", &mut led_driver);
        ct += 1;

        if ct > 720 {
            // refresh WiFi connection every 12 hours
            flash_pattern(". . _ . _ . _", &mut led_driver);
            match wifi_driver.disconnect() {
                Ok(_) => {
                    info!("WiFi driver disconnected.");
                    match wifi_driver.connect() {
                        Ok(_) => {
                            info!("WiFi driver reinitiating connection...");

                            while !wifi_driver.is_connected().unwrap() {
                                flash_pattern(". . _ _ _", &mut led_driver);
                                let config = wifi_driver.get_configuration().unwrap();
                                warn!("Waiting for station {:?}...", config);
                            }

                            info!("Connected.");
                        }
                        Err(e) => {
                            flash_pattern("_ _ .", &mut led_driver);
                            error!("WiFi driver connection initiation FAILED | {}", e);
                        }
                    };
                }
                Err(e) => {
                    flash_pattern("_ _ . .", &mut led_driver);
                    error!("WiFi driver connection reinitiation FAILED | {}", e);
                }
            };

            ct = 0;
        }

        let info = match wifi_driver.sta_netif().get_ip_info() {
            Ok(i) => i.ip.to_string(),
            Err(e) => {
                error!("WiFi check error | {}", e);
                flash_pattern("_ _ . _ ..", &mut led_driver);
                "0.0.0.0".to_string()
            }
        };

        if info != *"0.0.0.0" {
            let status_string = get(get_address, Some(api_secret));

            match serde_json::from_str::<DoorStatus>(&status_string) {
                Ok(mut ds) => {
                    flash_pattern(". . _ .  ", &mut led_driver);
                    if ds.executed == 0 {
                        flash_pattern(". . ___  ", &mut led_driver);
                        try_put = true;
                        let _ = led_driver.set_high();

                        let door_success = move_door(
                            ds.up,
                            ds.amount,
                            &mut motor_lead_1_driver,
                            &mut motor_lead_2_driver,
                        );

                        if door_success.is_some() {
                            while try_put {
                                flash_pattern(". . . ___  ", &mut led_driver);
                                ds.executed = 1;
                                match serde_json::to_string::<DoorStatus>(&ds) {
                                    Ok(ds_string) => {
                                        info!("DoorStatus | {}", ds_string);

                                        let put_string = put(
                                            put_address,
                                            api_secret,
                                            ds_string.as_bytes(),
                                            ds_string.len(),
                                        );
                                        match serde_json::from_str::<DoorStatus>(&put_string) {
                                            Ok(_) => {
                                                try_put = false;
                                            }
                                            Err(e) => {
                                                flash_pattern("_ _ ..___  ", &mut led_driver);
                                                error!("PUT response parse ERROR | {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        flash_pattern("_ _ ..__  ", &mut led_driver);
                                        error!("DoorStatus parse error | {}", e);
                                    }
                                }
                            }
                        } else {
                            flash_pattern("_ _ ....  ", &mut led_driver);
                        }

                        let _ = led_driver.set_low();
                    }
                }
                Err(e) => {
                    flash_pattern("_ _ .._.._  ", &mut led_driver);
                    error!("DoorStatus parse error | {}", e);
                }
            }
        } else {
            warn!("Waiting for connection...");
        }

        sleep(Duration::new(60, 0));
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

    info!("Response code: {}\n", status);

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

fn move_door(
    up: u8,
    amount: u8,
    ml1: &mut PinDriver<AnyOutputPin, esp_idf_hal::gpio::Output>,
    ml2: &mut PinDriver<AnyOutputPin, esp_idf_hal::gpio::Output>,
) -> Option<()> {
    info!(
        "Moving door {} by {}",
        if up == 1 { "up" } else { "down" },
        amount
    );

    if up == 1 {
        match ml1.set_high() {
            Ok(_) => (),
            Err(e) => {
                error!("ML1 set high FAILED | {}", e);
                return None;
            }
        }
    } else {
        match ml2.set_high() {
            Ok(_) => (),
            Err(e) => {
                error!("ML2 set high FAILED | {}", e);
                return None;
            }
        }
    }

    info!("Door moving...");

    sleep(Duration::new(amount as u64, 0));

    match ml1.set_low() {
        Ok(_) => (),
        Err(e) => {
            error!("ML1 set low FAILED | {}", e);
            return None;
        }
    }
    match ml2.set_low() {
        Ok(_) => (),
        Err(e) => {
            error!("ML2 set low FAILED | {}", e);
            return None;
        }
    }

    info!("Door stopped.");
    Some(())
}

fn flash_pattern(pattern: &str, led: &mut PinDriver<AnyOutputPin, esp_idf_hal::gpio::Output>) {
    let _ = led.set_low();
    for c in pattern.chars() {
        match c {
            '.' => {
                let _ = led.set_high();
                sleep(Duration::new(0, 100000000));
            }
            ' ' => {
                sleep(Duration::new(0, 300000000));
            }
            _ => {
                let _ = led.set_high();
                sleep(Duration::new(0, 300000000));
            }
        }

        let _ = led.set_low();
        sleep(Duration::new(0, 100000000));
    }
}
