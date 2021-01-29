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
    value: Integer,
}

async fn put_message(path, value) {
    // initiate logging
    env_logger::init();

    let path = mess;
    let value = value;
    println!("New zenoh...");
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Put Data ('{}': '{}')...\n", path, value);
    workspace
        .put(&path.try_into().unwrap(), value.into())
        .await
        .unwrap();

    // --- Examples of put with other types:

    // - Integer
    // workspace.put(&"/demo/example/Integer".try_into().unwrap(), 3.into())
    //     .await.unwrap();

    // - Float
    // workspace.put(&"/demo/example/Float".try_into().unwrap(), 3.14.into())
    //     .await.unwrap();

    // - Properties (as a Dictionary with str only)
    // workspace.put(
    //         &"/demo/example/Properties".try_into().unwrap(),
    //         Properties::from("p1=v1;p2=v2").into()
    //     ).await.unwrap();

    // - Json (str format)
    // workspace.put(
    //         &"/demo/example/Json".try_into().unwrap(),
    //         Value::Json(r#"{"kind"="memory"}"#.to_string()),
    //     ).await.unwrap();

    // - Raw ('application/octet-stream' encoding by default)
    // workspace.put(
    //         &"/demo/example/Raw".try_into().unwrap(),
    //         vec![0x48u8, 0x69, 0x33].into(),
    //     ).await.unwrap();

    // - Custom
    // workspace.put(
    //         &"/demo/example/Custom".try_into().unwrap(),
    //         Value::Custom {
    //             encoding_descr: "my_encoding".to_string(),
    //             data: vec![0x48u8, 0x69, 0x33].into(),
    //     }).await.unwrap();

    zenoh.close().await.unwrap();
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    app.at("/submit").post(|mut req: Request<()>| async move {
        let path: Path = req.body_json().await?;
        println!("Path: {}, Value: {}", path.path, path.value);

        if path.value != 1 {
            put_message(path.path, path.value).await?;
        } else {
            println!("value is not equal 1, can't publish path/value, path: {}, Value: {}", path.path, path.value);
        }

        Ok(Body::from_json(&cat)?)
    });

    app.listen("127.0.0.1:8080").await?;
    Ok(())
}