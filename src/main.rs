use futures::prelude::*;
use std::convert::TryInto;
use zenoh::*;

use clap::{App, Arg};

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    let path = "/demo/example/test";
    let value = "123";
    println!("Put Data ('{}': '{}')...\n", path, value);
    workspace
        .put(&path.try_into().unwrap(), value.into())
        .await
        .unwrap();

    println!("Get Data from {}'...\n", selector);
    let mut data_stream = workspace.get(&selector.try_into().unwrap()).await.unwrap();
    while let Some(data) = data_stream.next().await {
        println!(
            "  {} : {:?} (encoding: {} , timestamp: {})",
            data.path,
            data.value,
            data.value.encoding_descr(),
            data.timestamp
        )
    }

    zenoh.close().await.unwrap();
}

fn parse_args() -> (Properties, String) {
    let args = App::new("zenoh get example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(
            Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to get'")
                .default_value("/demo/example/test"),
        )
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = Properties::default();
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let selector = args.value_of("selector").unwrap().to_string();

    (config, selector)
}
