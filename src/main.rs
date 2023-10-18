use std::{net::SocketAddr, sync::Arc};

use bluest::{Adapter, AdvertisingDevice};
use futures_lite::stream::StreamExt;
use tokio::sync::{
    watch::{self, Sender},
    Mutex,
};
use warp::Filter;

#[tokio::main]
async fn main() {
    let (tx, rx) = watch::channel(0);
    let rx = Arc::new(Mutex::new(rx));

    tokio::spawn(scan_bluetooth(tx));

    let get_heart_rate = move || {
        let rx = rx.clone();
        async move {
            let mut rx = rx.lock().await;
            rx.changed().await.unwrap();
            let heart_rate = *rx.borrow();
            heart_rate.to_string()
        }
    };

    // GET /hello/warp => 200 OK with body "Hello, warp!"
    let root = warp::path::end().map(|| warp::reply::html(include_str!("../web/index.html")));
    let heartrate = warp::path!("heartrate").then(get_heart_rate);

    let socket_addr: SocketAddr = ([127, 0, 0, 1], 3030).into();
    println!("Start listening at: http://{socket_addr:?}");
    warp::serve(warp::get().and(root.or(heartrate)))
        .run(socket_addr)
        .await;
}

async fn scan_bluetooth(tx: Sender<i32>) {
    let adapter = Adapter::default()
        .await
        .ok_or("Bluetooth adapter not found")
        .unwrap();
    adapter.wait_available().await.unwrap();

    println!("Starting scan Xiaomi Band");
    let mut scan = adapter.scan(&[]).await.unwrap();

    while let Some(discovered_device) = scan.next().await {
        if let Some(heart_rate) = handle_device(discovered_device) {
            tx.send(heart_rate).unwrap();
        }
    }
}

fn handle_device(discovered_device: AdvertisingDevice) -> Option<i32> {
    let manufacturer_data = discovered_device.adv_data.manufacturer_data?;
    if manufacturer_data.company_id != 0x0157 {
        return None;
    }
    let name = discovered_device
        .device
        .name()
        .unwrap_or(String::from("(unknown)"));
    let rssi = discovered_device.rssi.unwrap_or_default();
    let heart_rate = manufacturer_data.data[3];
    println!("{name} ({rssi}dBm) Heart Rate: {heart_rate:?}",);
    Some(heart_rate.into())
}
