//
// 本程序的红绿灯适应于东南西北只有一个红绿灯，且只可直行，不可转弯
// 1. 读取配置文件
// 2. 根据配置文件，获取当前所有的红绿灯信息
//      1. 所有红绿灯等的信息，并发布到zenoh中
//      2. 南北和东西红绿灯的当前信息， 每隔1s发送一次
//

use std::fs;
use std::time::Duration;
use std::collections::HashMap;
// use async_std::task;
use yaml_rust::{YamlLoader};
use serde::{Deserialize, Serialize};
// use tide::prelude::*; // Pulls in the json! macro.
use tide::{Body, Request};
use zenoh::*;
use tokio;
use tokio::time::Instant;
use std::convert::TryInto;
// use lazy_static;
// #[macro_use]
extern crate lazy_static;
use lazy_static::lazy_static;
use std::sync::{Arc, Mutex};

//灯的颜色枚举
#[derive(Deserialize, Serialize, Debug, Clone)]
enum LightColor {
    UNKNOWN = 0,
    GREEN = 1,
    RED = 2,
    YELLOW = 3,
}

#[derive(Debug, Clone)]
struct Light {
    id: String,
    color: LightColor,
}

#[derive(Debug, Clone)]
struct LightDuration {
    green: i64,
    red: i64,
    yellow: i64,
    unknown: i64
}

#[derive(Deserialize, Serialize, Debug)]
struct RuleMessage {
    name: String,
    color: i32,
    duration: i64,
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

#[derive(Debug, Clone)]
struct ControlInfo {
    road_id: String,
    light_name: String,
    light_color: LightColor,
    duration: i64,
}

// 主灯的状态，包括当前颜色，和倒计时
#[derive(Debug, Clone)]
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

lazy_static! {
    // 所有灯的状态
    static ref LIGHTSTATUS:Mutex<HashMap<String, LightStatus>> = {
        let mut lgt_status = HashMap::new();
        Mutex::new(lgt_status)
    };

    // 公用的灯循环的时间配置
    static ref LIGHTDURATION:Mutex<LightDuration> = {
        let mut lgt_drtion = LightDuration{
            green: 0,
            red: 0,
            yellow: 0,
            unknown: 0
        };
        Mutex::new(lgt_drtion)
    };
}


// 根据灯色获取时长
fn get_duration(color: &LightColor) -> i64{
    let lcfg = LIGHTDURATION.lock().unwrap();
    match color {
        &LightColor::RED => lcfg.red,
        &LightColor::GREEN => lcfg.green,
        &LightColor::YELLOW => lcfg.yellow,
        &LightColor::UNKNOWN => lcfg.unknown,
        _ => lcfg.unknown,
    }
}

// 获取相反的灯色
fn inverse_color(color: &LightColor, counter: i64) -> LightColor {
    let current_color = color.clone(); 
    let lcfg = LIGHTDURATION.lock().unwrap();
    match current_color {
        LightColor::RED => (|| {
            if counter > lcfg.yellow {
                LightColor::GREEN
            } else {
                LightColor::YELLOW
            }
        })(),
        LightColor::GREEN => (|| {
            LightColor::RED
           
        })(),
        LightColor::YELLOW => (|| {
            LightColor::RED
           
        })(),
        LightColor::UNKNOWN =>  (|| {
            LightColor::UNKNOWN
           
        })(),
        _ => (|| {
            LightColor::UNKNOWN
           
        })(),
    }
}
// 根据配置，给LIGHTDURATION赋值
fn init_light_duration(init_color: &LightColor, counter: i64) {
    let color = init_color.clone();
    let lcfg = LIGHTDURATION.lock().unwrap();
    match color {
        LightColor::GREEN => lcfg.green = counter,
        LightColor::RED => lcfg.red = counter,
        LightColor::YELLOW => lcfg.yellow = counter,
        LightColor::UNKNOWN => lcfg.unknown = counter,
    };
}

// 根据配置计算所有灯的初始状态status
fn init_lgt_status(lgt_id: String, init_color: LightColor, groups: &Vec<[String;2]>){
    let group1 = groups[0].clone();
    let group2 = groups[1].clone();
    let lgt_id1 = group1[0];
    let lgt_id2 = group1[1];
    let lgt_id3 = group2[0];
    let lgt_id4 = group2[1];
    let counter = get_duration(&init_color);
    let in_color = inverse_color(&init_color, counter);  // 反转的颜色
    let in_counter = get_duration(&in_color);  // 反转的时长
    let mut lgt_status = LIGHTSTATUS.lock().unwrap();
    // 获取灯色对应的时长
    if lgt_id == lgt_id1 || lgt_id == lgt_id2 {
        lgt_status.insert(lgt_id1, 
            LightStatus{color: init_color, counter: counter});
        lgt_status.insert(lgt_id2, 
            LightStatus{color: init_color, counter: counter});
        lgt_status.insert(lgt_id3, 
            LightStatus{color: in_color, counter: in_counter});
        lgt_status.insert(lgt_id4, 
            LightStatus{color: in_color, counter: in_counter});
    } else {
        lgt_status.insert(lgt_id1, 
            LightStatus{color: in_color, counter: in_counter});
        lgt_status.insert(lgt_id2, 
            LightStatus{color: in_color, counter: in_counter});
        lgt_status.insert(lgt_id3, 
            LightStatus{color: init_color, counter: counter});
        lgt_status.insert(lgt_id4, 
            LightStatus{color: init_color, counter: counter});
    }
}


async fn light_loop(road_id: String, light_group:HashMap<String, Vec<String>>) {
    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    //每秒tick
    loop {
        let now = Instant::now();
        {
            let lgt_status_list = LIGHTSTATUS.lock().unwrap();
            let lgt_duration = LIGHTDURATION.lock().unwrap();

            for (group_name, lgt_status) in lgt_status_list.into_iter() {
                if lgt_status.tick(&lgt_duration) {
                    // 根据group_name,获取Light
                    let light_list: &Vec<String> = light_group.get(&group_name).unwrap();
                    
                    // 更新Zenoh中的存储
                    let mut path = "/light/detail/".to_string();
                    path += &road_id;
                    // let path = Selector::new(path.to_string());

                    // 1. 取出原有数据
                    // let selector = path;
                    // let lgt_now = workspace.get(&path.try_into().unwrap()).await.unwrap();
                    // println!("{:?}", lgt_now);

                    // 2. 更新存储的灯的详细信息
                    // path: /light/detail/{road_id}
                    // value: {"id_1": 1, "id_2": 1, "id_3": 2, "id_4": 2,}  light_id: color
                    let mut value = String::from("{");
                    for light_id in light_list {
                        value = format!("{}\"{}\":\"{:?}\",", value, light_id, lgt_status.color);
                    }
                    value += &String::from("}");
                    println!("Put Data ('{}': '{}')...\n", path, value);
                    workspace.put(
                                &path.try_into().unwrap(),
                                Value::Json(value),
                            ).await.unwrap();
                }
            }
        }
        
        // 2. 更新存储的灯的剩余时间
        // path: /road_id/left
        // value: [{"light_id": "12", "color": 1, "remain": 5}]

        tokio::time::sleep_until(now.checked_add(Duration::from_secs(1)).unwrap()).await;
        // if light_status.tick(&LIGHTDURATION) {
            // let now = Instant::now();
            // println!("{:?}", now);
            // // 如果灯有变化，计算所有灯的信息，并将信息发布到zenoh
            // // 发布信息分为2部分，1： 所有灯的具体信息 2： 发送给控制台的红绿灯信息
            // // 1. 计算其他红绿灯信息
            // let mut is_change = false;
            // if light_status.counter == 3 {
            //     is_change = true;
            // }
            // let color = &light_status.color;
            // println!("{:?}", color);

            // let light_info = cal_light_info(&light_hash, &master, &color, &groups, &is_change);
            // println!("{:?}", light_info);
            // 2. 信息发送到zenoh
            // pub_light2(&workspace, String::from("road"), &light_info).await;
            
            // let path = format!("/light/detail/{}", road_id);
            // let mut value = String::from("{");
            // for light in light_info.iter() {
            //     value = format!("{}\"{}\":\"{:?}\",", value, light.id.to_string(), light.color);

            // }
            // value += &String::from("}");
            // println!("Put Data ('{}': '{}')...\n", path, value);
            // // Value::Json(r#"{"kind"="memory"}"#.to_string()),
            // workspace.put(
            //             &path.try_into().unwrap(),
            //             Value::Json(value),
            //         ).await.unwrap();


            // 3. 发布方位红绿灯信息到zenoh
            // pub_light3(String::from("road"), &light_info).await;
            // let path = format!("/light/state/{}", road_id);
            // let mut value = String::from("{");
            // for light in light_info.iter() {
            //     value = format!("{}\"{}\":\"{:?}\",", value, light.name.to_string(), light.color);

            // }
            // value += &String::from("}");
            // println!("Put Data ('{}': '{}')...\n", path, value);
            // Value::Json(r#"{"kind"="memory"}"#.to_string()),
            // workspace.put(
            //             &path.try_into().unwrap(),
            //             Value::Json(value),
            //         ).await.unwrap();
            // //http通知，这里通知自己
            // let mut url = "http://127.0.0.1:8080/show/".to_owned();
            // url.push_str(light_status.desc());
            // //这里使用异步的reqwest库
            // let body = reqwest::get(&url[..]).await;
        // }
        //注意这个sleep是异步的，不影响其它操作
        
    }
    // zenoh.close().await.unwrap();
}


fn read_config(file_name: &str) -> (String, HashMap<String, Vec<String>>) {
    let config_str = fs::read_to_string(file_name).unwrap();
    let config_docs = YamlLoader::load_from_str(config_str.as_str()).unwrap();
    let config = &config_docs[0];
    let light_group_cfg = &config["light_id_group"];
    let road_id =  String::from(config["road_id"].as_str().unwrap());

    // 读取灯的变化时间
    let light_duration = LIGHTDURATION.lock().unwrap();
    light_duration.green = config["duration"]["green"].as_i64().unwrap();
    light_duration.red = config["duration"]["red"].as_i64().unwrap();
    light_duration.yellow = config["duration"]["yellow"].as_i64().unwrap();
    light_duration.unknown = config["duration"]["unknown"].as_i64().unwrap();

    // 读取配置中的红绿灯颜色
    let default_color:LightColor;
    match config["color"].as_i64().unwrap() {
        1 => default_color = LightColor::RED,
        2 => default_color = LightColor::GREEN,
        3 => default_color = LightColor::YELLOW,
        0 => default_color = LightColor::UNKNOWN,
        _ => default_color = LightColor::RED,
    }
    let init_duration = get_duration(&default_color);

    // 红绿灯组
    let group_master = config["master"].as_str().unwrap();
    let mut lgt_status_group_hash = LIGHTSTATUS.lock().unwrap();
    let mut light_group:HashMap<String, Vec<String>> = HashMap::new();

    // 读取配置中的红绿灯组
    for (group_name, lgt_id_list) in light_group_cfg.as_hash().unwrap().into_iter() {
        let group_name = String::from(group_name.as_str().unwrap());
        let mut g_id_list = vec![];
        for lgt_id in lgt_id_list.as_vec().unwrap() {
            g_id_list.push(String::from(lgt_id.as_str().unwrap()));
        }
        light_group.insert(group_name, g_id_list);

        // 初始化LIGHTSTATUS
        if group_name == group_master {
            lgt_status_group_hash.insert(group_name, LightStatus{color: default_color, counter: init_duration});
        } else {
            let in_color = inverse_color(&default_color, init_duration);
            let in_duration = get_duration(&default_color);
            lgt_status_group_hash.insert(group_name, LightStatus{color: in_color, counter: in_duration});
        }
    }
    (road_id, light_group)
}

//http服务，处理修改配置的请求
async fn serve_http() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    // 红绿灯规则调整
    app.at("/rule_change").post(|mut req: Request<()>| async move {
        let rule: RuleMessage = req.body_json().await?;
        // println!("Message: {}", rule.name);
        // 这里修改全局变量


        //返回一个没用的response
        let response =  Response {status: 1, message: String::from("")};

        Ok(Body::from_json(&response)?)
    });

    //http server启动
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}




#[tokio::main]
async fn main() {
    // 1. 读取配置文件
    let f = String::from("/home/duan/study/src/default.yaml");
    let (road_id, light_group) = read_config(&f);
    println!("{:?}{:?}", road_id, light_group);
    
    // 循环红绿灯
    tokio::spawn(light_loop(road_id, light_group));

    // 启动服务主线程自己阻塞在serve_http循环上
    use futures::executor::block_on;
    block_on(serve_http());

}

