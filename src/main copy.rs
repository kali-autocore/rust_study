//
// 本程序的红绿灯适应于东南西北只有一个红绿灯，且只可直行，不可转弯
// 1. 读取配置文件
// 2. 根据配置文件，获取当前所有的红绿灯信息
//      1. 所有红绿灯等的信息，并发布到zenoh中
//      2. 南北和东西红绿灯的当前信息， 每隔1s发送一次
//

use std::fs;
use std::{thread, time};
use std::time::Duration;
use async_std::task;
use yaml_rust::{YamlLoader};
use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};
// use tide::prelude::*; // Pulls in the json! macro.
use tide::{Body, Request};
use std::convert::TryInto;
use std::thread::spawn;
use zenoh::*;

//灯的颜色枚举
#[derive(Debug, Clone)]
enum LightColor {
    GREEN = 1,
    RED = 2,
    YELLOW = 3,
}

#[derive(Debug, Clone)]
struct Light {
    id: String,
    name: String,
    color: LightColor,
}

#[derive(Debug, Clone)]
struct LightDuration {
    green: i64,
    red: i64,
    yellow: i64
}

#[derive(Debug)]
struct Message {
    path: String,
    value: Light,
}

#[derive(Deserialize, Serialize)]
struct Response {
    status: i32,
    message: String,
}


#[derive(Debug, Clone)]
struct Config {
    road_id: String,
    master: String,
    init_color: LightColor,
    groups: Vec<[String;2]>,
    light_duration: LightDuration
}


// 主灯的状态，包括当前颜色，和倒计时
struct LightStatus {
    color: LightColor,
    counter: i64,
}


// 灯状态的实现
impl LightStatus {
    // 转灯，每个tick（1秒）调用一次，如果倒计时结束就转灯，并返回true；否则返回false
    fn tick(&mut self, light_duration: &LightDuration) -> bool {
        println!("{:?}",self.counter);
        self.counter -= 1;
        if self.counter == 3 {
            match self.color {
                LightColor::RED => (|| {
                    true
                })(),
                _ => false
            }
        } else if self.counter == 0 {
            match self.color {
                LightColor::GREEN => (|| {
                    self.color = LightColor::YELLOW;
                    self.counter = light_duration.yellow;
                })(),
                LightColor::YELLOW => (|| {
                    self.color = LightColor::RED;
                    self.counter = light_duration.red;
                })(),
                LightColor::RED => (|| {
                    self.color = LightColor::GREEN;
                    self.counter = light_duration.green;
                })()
            };
            true
        } else {
            false
        }
    }

    //返回当前状态的可读描述
    fn desc(&self) -> &str {
        match self.color {
            LightColor::GREEN => "GREEN",
            LightColor::YELLOW => "YELLOW",
            LightColor::RED => "RED",
            _ => ""
        }
    }
}

async fn light_loop(light_hash:&LinkedHashMap<String, Light>, cfg:Config) {
    let light_duration = cfg.light_duration;
    let road_id = cfg.road_id;
    let groups = cfg.groups;  // 
    let master = cfg.master;
    let init_color = cfg.init_color;

    // 灯的初始状态：红灯，满血
    let mut light_status = LightStatus { color: init_color.clone(), counter: light_duration.red };
    

    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    loop {
        //每秒一个tick
        if light_status.tick(&light_duration) {
            // 如果灯有变化，计算所有灯的信息，并将信息发布到zenoh
            // 发布信息分为2部分，1： 所有灯的具体信息 2： 发送给控制台的红绿灯信息
            // 1. 计算其他红绿灯信息
            let mut is_change = false;
            if light_status.counter == 3 {
                is_change = true;
            }
            let color = &light_status.color;
            println!("{:?}", color);

            let light_info = cal_light_info(&light_hash, &master, &color, &groups, &is_change);
            println!("{:?}", light_info);
            // 2. 信息发送到zenoh
            // pub_light2(&workspace, String::from("road"), &light_info).await;
            
            let path = format!("/light/detail/{}", road_id);
            let mut value = String::from("{");
            for light in light_info.iter() {
                value = format!("{}\"{}\":\"{:?}\",", value, light.id.to_string(), light.color);

            }
            value += &String::from("}");
            println!("Put Data ('{}': '{}')...\n", path, value);
            // Value::Json(r#"{"kind"="memory"}"#.to_string()),
            workspace.put(
                        &path.try_into().unwrap(),
                        Value::Json(value),
                    ).await.unwrap();


            // 3. 发布方位红绿灯信息到zenoh
            // pub_light3(String::from("road"), &light_info).await;
            let path = format!("/light/state/{}", road_id);
            let mut value = String::from("{");
            for light in light_info.iter() {
                value = format!("{}\"{}\":\"{:?}\",", value, light.name.to_string(), light.color);

            }
            value += &String::from("}");
            println!("Put Data ('{}': '{}')...\n", path, value);
            // Value::Json(r#"{"kind"="memory"}"#.to_string()),
            workspace.put(
                        &path.try_into().unwrap(),
                        Value::Json(value),
                    ).await.unwrap();
            // //http通知，这里通知自己
            // let mut url = "http://127.0.0.1:8080/show/".to_owned();
            // url.push_str(light_status.desc());
            // //这里使用异步的reqwest库
            // let body = reqwest::get(&url[..]).await;
        }
        //注意这个sleep是异步的，不影响其它操作
        task::sleep(Duration::from_secs(1)).await;
    }
    // zenoh.close().await.unwrap();
}


fn cal_color (color: &LightColor, change: &bool) -> LightColor {
    let mut other_color = LightColor::RED;
    match color {
        LightColor::GREEN => (|| {
            other_color = LightColor::RED;
        })(),
        LightColor::YELLOW => (|| {
            other_color = LightColor::RED;
            
        })(),
        LightColor::RED => (|| {
            other_color = LightColor::GREEN;
            if change == &true {
                other_color = LightColor::YELLOW;
            }
            
        })()
    }
    other_color
}

// 根据master红绿灯信息计算其他红绿灯信息
fn cal_light_info(light_hash: &LinkedHashMap<String, Light>, master: &String, color: &LightColor, 
groups: &Vec<[String;2]>, change: &bool) -> Vec<Light> {
    let mut light_info = vec![];

    let group1 = &groups[0];
    let group2 = &groups[1];
    let mut lgt1 = light_hash.get(&group1[0]).unwrap().clone();
    let mut lgt2 = light_hash.get(&group1[1]).unwrap().clone();
    let mut lgt3 = light_hash.get(&group2[0]).unwrap().clone();
    let mut lgt4 = light_hash.get(&group2[1]).unwrap().clone();

    if master == &group1[0] || master == &group1[1] {
        for (name, _) in light_hash.into_iter() {
            if name == &group1[0] || name == &group1[1] {
                
                lgt1.color = color.clone();
                lgt2.color = color.clone();
                let other_color = cal_color(color, change);
                lgt3.color = other_color.clone();
                lgt4.color = other_color.clone();
                light_info.push(lgt1);
                light_info.push(lgt2);
                light_info.push(lgt3);
                light_info.push(lgt4);
                break;
            }
        }
        
    }
    light_info
}


fn read_config(file_name: &str) -> (Vec<Light>, LinkedHashMap<String, Light>, Config) {
    let config_str = fs::read_to_string(file_name).unwrap();
    let config_docs = YamlLoader::load_from_str(config_str.as_str()).unwrap();
    let config = &config_docs[0];
    let light_id = &config["light_id"];
    let default_color:LightColor;
    
    // 转成灯颜色枚举对象
    match config["color"].as_str().unwrap() {
        "GREEN" => default_color = LightColor::GREEN,
        "RED" => default_color = LightColor::RED,
        "YELLOW" => default_color = LightColor::YELLOW,
        _ => default_color = LightColor::RED,
    }

    let mut light_hash = LinkedHashMap::new();
    let mut light:Vec<Light> = vec![];
    for (name, id) in light_id.as_hash().unwrap().into_iter() {
        let nm = String::from(name.as_str().unwrap());
        let lgt = Light {
            id: String::from(id.as_str().unwrap()),
            name: nm.clone(),
            color: default_color.clone()
        };
        light.push(lgt.clone());
        light_hash.insert(nm, lgt);
    }

    let group_cfg = &config["groups"];
    let mut groups = vec![];
    for (key, value) in group_cfg.as_hash().unwrap().into_iter() {
        groups.push([String::from(key.as_str().unwrap()), String::from(value.as_str().unwrap())]);
    }

    let config = Config{
        road_id: String::from(config["road_id"].as_str().unwrap()),
        master: String::from(config["master"].as_str().unwrap()),
        init_color: default_color,
        groups: groups,
        light_duration: LightDuration{
            green: config["duration"]["green"].as_i64().unwrap(),
            red: config["duration"]["red"].as_i64().unwrap(),
            yellow: config["duration"]["yellow"].as_i64().unwrap()
        }
    };
    (light, light_hash, config)
}


// #[async_std::main]
// async fn main() -> tide::Result<()> {
//     tide::log::start();
//     let mut app = tide::new();
    
//     app.at("/light").post(|mut req: Request<()>| async move{
//         let message: Message = req.body_json().await?;
//         println!("Message: {}", message.path);
//         let path = format!("{}/{}", message.path, message.value.id.to_string());
//         let value = format!("{}", message.value.state);
//         pub_message(path, value).await;
//         let ret_message = Response {status: 1, message: String::from("")};
//         Ok(Body::from_json(&ret_message)?)
//     });
//     ss();

//     app.listen("127.0.0.1:8080").await?;
//     Ok(())
// }

#[async_std::main]
async fn main() {
    // 1. 读取配置文件
    let f = String::from("/home/duan/study/src/default.yaml");
    let (light_list, light_hash, cfg) = read_config(&f);
    println!("{:?}{:?}{:?}", light_list, light_hash, cfg);
    // 2. 1s循环一次，并发送红绿灯信息到管控中心；红绿灯变化时，发布信息到zenoh中，存储红绿灯信息
    light_loop(&light_hash, cfg).await;
    
    // 启动服务，监听管控中心发送的转换红绿灯信息
    let mut aa = 1;
    let bb = &a;
    let cc = &a;
    println!("{:?},{:?}", bb, cc);
    aa = 2;
    println!("{:?},{:?}", bb, cc);


}
