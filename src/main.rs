use std::error::Error;

use bluest::{Adapter, AdvertisingDevice};
use futures_lite::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let adapter = Adapter::default()
        .await
        .ok_or("Bluetooth adapter not found")?;
    adapter.wait_available().await?;

    println!("starting scan");
    let mut scan = adapter.scan(&[]).await?;

    println!("scan started");
    while let Some(discovered_device) = scan.next().await {
        handle_device(discovered_device)
    }
    Ok(())
}

fn handle_device(discovered_device: AdvertisingDevice) {
    if let Some(manufacturer_data) = discovered_device.adv_data.manufacturer_data {
        if manufacturer_data.company_id != 0x0157 {
            return;
        }
        let name = discovered_device
            .device
            .name()
            .unwrap_or(String::from("(unknown)"));
        let rssi = discovered_device.rssi.unwrap_or_default();
        let heart_rate = manufacturer_data.data[3];
        println!("{name} ({rssi}dBm) Heart Rate: {heart_rate:?}",);
    }
}
