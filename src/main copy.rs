use serde::{Deserialize, Serialize};
// use tide::prelude::*; // Pulls in the json! macro.
use tide::{Body, Request};
use std::convert::TryInto;
use zenoh::*;

#[derive(Deserialize, Serialize)]
struct Light {
    id: i32,
    state: String,
}

#[derive(Deserialize, Serialize)]
struct Message {
    path: String,
    value: Light,
}

#[derive(Deserialize, Serialize)]
struct Response {
    status: i32,
    message: String,
}

// publish message to zenoh
async fn pub_message(path:String, value:String) {
    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Put Data ('{}': '{}')...\n", path, value);
    workspace
        .put(&path.try_into().unwrap(), value.into())
        .await
        .unwrap();
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    app.at("/light").post(|mut req: Request<()>| async move{
        let message: Message = req.body_json().await?;
        println!("Message: {}", message.path);
        let path = format!("{}/{}", message.path, message.value.id.to_string());
        let value = format!("{}", message.value.state);
        pub_message(path, value).await;
        let ret_message = Response {status: 1, message: String::from("")};
        Ok(Body::from_json(&ret_message)?)
    });

    app.listen("127.0.0.1:8080").await?;
    Ok(())
}