// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use futures_util::{self, pin_mut, stream::StreamExt};
use kitsune_p2p_mdns::*;

#[tokio::main]
async fn main() {
    println!("Starting discovery");
    let service_name = "bobby".to_owned();
    // Start Stream
    let stream = mdns_listen(service_name);
    pin_mut!(stream);
    while let Some(maybe_response) = stream.next().await {
        match maybe_response {
            Ok(response) => {
                println!("Discovered: {:?}", response);
            }
            Err(e) => {
                println!("!!! Discovery Error: {:?}", e);
            }
        }
    }
}
