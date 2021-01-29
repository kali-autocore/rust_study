// 1. 启动一个服务
// 2. 等待对方发送POST请求
// 3. 当请求参数a的值为1的时候，使用zenoh put 信息

use serde::{Deserialize, Serialize};
use tide::prelude::*; // Pulls in the json! macro.
use tide::{Body, Request};
use std::convert::TryInto;
use zenoh::*;

#[derive(Deserialize, Serialize)]
struct Path {
    path: String,
    value: u32,
}

async fn put_message(path: String, value:u32) {
    // initiate logging
    // env_logger::init();
    println!("Enter put_message");
    let path = path;
    let value = value;

    let mut config = Properties::default();
    // for key in ["mode", "peer", "listener"].iter() {
    //     config.insert(key.to_string(), key.to_string());
    // }

    println!("New zenoh...");
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Put Data ('{}': '{}')...\n", path, value);
    workspace
        .put(&path.try_into().unwrap(), value.to_string().into())
        .await
        .unwrap();
    zenoh.close().await.unwrap();
    ()
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    app.at("/submit").post(|mut req: Request<()>| async move {
        let path: Path = req.body_json().await?;
        println!("Path: {}, Value: {}", path.path, path.value);
        let p = path.path;
        let v = path.value;
        if p == "/demo/example/test" {
            put_message(p, v).await;
        } else {
            println!("value is not equal 1, can't publish path/value, path: {}, Value: {}", &p, &v);
        }


        Ok(Body::from_json(&9)?)
    });

    app.listen("127.0.0.1:8080").await?;
    Ok(())
}