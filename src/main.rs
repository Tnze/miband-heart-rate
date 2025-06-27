use std::error::Error;
use std::net::SocketAddr;

use bluest::{btuuid::bluetooth_uuid_from_u16, Adapter, Device, Uuid};
use futures_lite::stream::StreamExt;
use serde::Serialize;
use tokio::sync::watch::{self, Receiver, Sender};
use warp::Filter;

const HRS_UUID: Uuid = bluetooth_uuid_from_u16(0x180D);
const HRM_UUID: Uuid = bluetooth_uuid_from_u16(0x2A37);

#[tokio::main]
async fn main() {
    let (tx, rx) = watch::channel(HeartRate {
        value: 0,
        sensor_contact: None,
    });
    let result = tokio::join!(ble_scanner(tx), web_server(rx));
    println!("{result:?}");
}

#[derive(Serialize)]
struct HeartRate {
    value: u16,
    sensor_contact: Option<bool>,
}

async fn web_server(rx: Receiver<HeartRate>) -> Result<(), Box<dyn Error>> {
    let root = warp::path::end().map(|| warp::reply::html(include_str!("../web/index.html")));
    let heartrate = warp::path!("heartrate").then(move || {
        let mut rx = rx.clone();
        async move {
            drop(rx.borrow_and_update());
            rx.changed().await.unwrap();
            warp::reply::json(&rx.borrow().value)
        }
    });

    let socket_addr: SocketAddr = ([127, 0, 0, 1], 3030).into();
    println!("Start listening at http://{socket_addr:?}");

    warp::serve(warp::get().and(root).or(heartrate))
        .run(socket_addr)
        .await;
    Err("Server stopped".into())
}

async fn ble_scanner(tx: Sender<HeartRate>) -> Result<(), Box<dyn Error>> {
    let adapter = Adapter::default()
        .await
        .ok_or("Bluetooth adapter not found")
        .unwrap();
    adapter.wait_available().await.unwrap();

    loop {
        let device = {
            let connected_heart_rate_devices =
                adapter.connected_devices_with_services(&[HRS_UUID]).await?;
            if let Some(device) = connected_heart_rate_devices.into_iter().next() {
                device
            } else {
                println!("Starting scan");
                let mut scan = adapter.discover_devices(&[HRS_UUID]).await?;

                println!("Scan started");
                let device = scan.next().await.unwrap()?;

                println!("Found Device: [{}] {:?}", device, device.name_async().await);
                device
            }
        };

        if let Err(err) = handle_device(&adapter, &device, &tx).await {
            println!("Connection error: {err:?}");
        }
    }
}

async fn handle_device(
    adapter: &Adapter,
    device: &Device,
    tx: &Sender<HeartRate>,
) -> Result<(), Box<dyn Error>> {
    // Connect
    if !device.is_connected().await {
        println!("Connecting device: {}", device.id());
        adapter.connect_device(&device).await?;
    }

    // Discover services
    let heart_rate_services = device.discover_services_with_uuid(HRS_UUID).await?;
    let heart_rate_service = heart_rate_services
        .first()
        .ok_or("Device should has one heart rate service at least")?;

    // Discover
    let heart_rate_measurements = heart_rate_service
        .discover_characteristics_with_uuid(HRM_UUID)
        .await?;
    let heart_rate_measurement = heart_rate_measurements
        .first()
        .ok_or("HeartRateService should has one heart rate measurement characteristic at least")?;

    let mut updates = heart_rate_measurement.notify().await?;
    while let Some(Ok(heart_rate)) = updates.next().await {
        let flag = *heart_rate.get(0).ok_or("No flag")?;

        // Heart Rate Value Format
        let mut heart_rate_value = *heart_rate.get(1).ok_or("No heart rate u8")? as u16;
        if flag & 0b00001 != 0 {
            heart_rate_value |= (*heart_rate.get(2).ok_or("No heart rate u16")? as u16) << 8;
        }

        // Sensor Contact Supported
        let mut sensor_contact = None;
        if flag & 0b00100 != 0 {
            sensor_contact = Some(flag & 0b00010 != 0)
        }
        println!("HeartRateValue: {heart_rate_value}, SensorContactDetected: {sensor_contact:?}");
        tx.send(HeartRate {
            value: heart_rate_value,
            sensor_contact,
        })?;
    }
    Err("No longer heart rate notify".into())
}
