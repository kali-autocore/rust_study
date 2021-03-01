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
use std::collections::HashMap;
// use async_std::task;
use async_std::task::JoinHandle;
use yaml_rust::{YamlLoader};
use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};
// use tide::prelude::*; // Pulls in the json! macro.
use tide::{Body, Request};
use std::convert::TryInto;
use zenoh::*;
use tokio;
use tokio::task;
use tokio::time::Instant;
// use once_cell::sync::Lazy;
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
    name: String,
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
    static ref LIGHTSTATUS:HashMap<String, LightStatus> = {
        let mut lgt_status = HashMap::new();
        lgt_status
    };

    // 公用的灯循环的时间配置
    static ref LIGHTDURATION:LightDuration = {
        let mut lgt_drtion = LightDuration{green: 0,
            red: 0,
            yellow: 0,
            unknown: 0
        };
        lgt_drtion
    };
}

// 根据灯色获取时长
fn get_duration(color: &LightColor) -> i64{
    match color {
        &LightColor::RED => LIGHTDURATION.red,
        &LightColor::GREEN => LIGHTDURATION.green,
        &LightColor::YELLOW => LIGHTDURATION.yellow,
        &LightColor::UNKNOWN => LIGHTDURATION.unknown,
        _ => LIGHTDURATION.unknown,
    }
}

// 获取相反的灯色
fn inverse_color (color: &LightColor, counter: i64) -> LightColor {
    let current_color = color.clone(); 
    match current_color {
        LightColor::RED => (|| {
            if counter > LIGHTDURATION.yellow {
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
    match color {
        LightColor::GREEN => LIGHTDURATION.green = counter,
        LightColor::RED => LIGHTDURATION.red = counter,
        LightColor::YELLOW => LIGHTDURATION.yellow = counter,
        LightColor::UNKNOWN => LIGHTDURATION.unknown = counter,
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

    // 获取灯色对应的时长
    if lgt_id == lgt_id1 || lgt_id == lgt_id2 {
        LIGHTSTATUS.insert(lgt_id1, 
            LightStatus{color: init_color, counter: counter});
        LIGHTSTATUS.insert(lgt_id2, 
            LightStatus{color: init_color, counter: counter});
        LIGHTSTATUS.insert(lgt_id3, 
            LightStatus{color: in_color, counter: in_counter});
        LIGHTSTATUS.insert(lgt_id4, 
            LightStatus{color: in_color, counter: in_counter});
    } else {
        LIGHTSTATUS.insert(lgt_id1, 
            LightStatus{color: in_color, counter: in_counter});
        LIGHTSTATUS.insert(lgt_id2, 
            LightStatus{color: in_color, counter: in_counter});
        LIGHTSTATUS.insert(lgt_id3, 
            LightStatus{color: init_color, counter: counter});
        LIGHTSTATUS.insert(lgt_id4, 
            LightStatus{color: init_color, counter: counter});
    }
}

async fn light_loop(light_id:String, cfg:Config) {
    let light_duration = cfg.light_duration;
    let road_id = cfg.road_id;
    let groups = cfg.groups;  
    let master = cfg.master;
    let init_color = cfg.init_color;

    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();
    let light_status: &LightStatus = LIGHTSTATUS.get(&light_id).unwrap();

    loop {
        let now = Instant::now();
             
        //每秒一个tick
        if light_status.tick(&light_duration) {
            let now = Instant::now();
            println!("{:?}", now);
            // 如果灯有变化，计算所有灯的信息，并将信息发布到zenoh
            // 发布信息分为2部分，1： 所有灯的具体信息 2： 发送给控制台的红绿灯信息
            // 1. 计算其他红绿灯信息
            let mut is_change = false;
            if light_status.counter == 3 {
                is_change = true;
            }
            let color = &light_status.color;
            println!("{:?}", color);

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
        }
        //注意这个sleep是异步的，不影响其它操作
        tokio::time::sleep_until(now.checked_add(Duration::from_secs(1)).unwrap()).await;
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
fn cal_light_info(light_hash: &HashMap<String, Light>, master: &String, color: &LightColor, 
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


fn read_config(file_name: &str) -> (Vec<Light>, HashMap<String, Light>, Config) {
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

    let mut light_hash = HashMap::new();
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
            yellow: config["duration"]["yellow"].as_i64().unwrap(),
            unknown: 0
        }
    };
    (light, light_hash, config)
}


static mut RESET: bool = false;
lazy_static! {
    static ref LIGHTHASH: HashMap<String, Light> = {
        HashMap::new()
    };
    static ref LIGHTCONFIG: Config = {
        Config{
                road_id: String::from(""),
                master: String::from(""),
                init_color: LightColor::GREEN,
                groups: vec![],
                light_duration: LightDuration { green: 7,
                    red: 10,
                    yellow: 3,
                    unknown: 0
                }
            }
    };
    static ref HASHMAP: HashMap<u32, &'static str> = {
        let mut m = HashMap::new();
        m.insert(0, "foo");
        m.insert(1, "bar");
        m.insert(2, "baz");
        m
    };
        
}
   
// static ref LIGHTCONFIG: Lazy<Config> = Lazy::new(||{
//     Config{
//         road_id: String::from(""),
//         master: String::from(""),
//         init_color: LightColor::GREEN,
//         groups: vec![],
//         light_duration: LightDuration { green: 7,
//             red: 10,
//             yellow: 3
//         }
//     }
// });
// lazy_static! {
//     static ref HASHMAP: Mutex<HashMap<u32, &'static str>> = {
//         let mut m = HashMap::new();
//         m.insert(0, "foo");
//         Mutex::new(m)
//     };
// }

// http服务，接收CloudViewer请求，更改红绿灯状态和时长
async fn serve_http() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    // #[derive(Deserialize, Serialize, Debug)]
    // struct RuleMessage {
    //     name: String,
    //     color: i32,
    //     duration: i64,
    // }
    // 更改红绿灯规则POST请求
    app.at("/rule_chg").post(|mut req: Request<()>| async move {
        let rule: RuleMessage  = req.body_json().await?;
        println!("rule message: {:?}", rule);

        // 按照请求要求，调整配置
        unsafe {
            match cmd.key.as_ref() {
                "green"  => LIGHT_CONFIG.green_duration = cmd.value,
                "yellow"  => LIGHT_CONFIG.yellow_duration = cmd.value,
                "red"  => LIGHT_CONFIG.red_duration = cmd.value,
                _ => ()
            }
        }

        //返回一个没用的response
        let response = Response {
            message: "done".to_string(),
        };

        Ok(Body::from_json(&response)?)
    });

    //接收通知，本来是通知第三方的，测试阶段，自己就是个web server，所以这里通知自己
    app.at("/show/:color").get(|mut req: Request<()>| async move {
        //输出通知的内容
        println!("NOTIFY: {:?}", req.param("color"));
        Ok(json!({
            "Result": "Ok",
        }))
    });
    //http server启动
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}

#[async_std::main]
async fn main() {
    // 1. 读取配置文件
    let f = String::from("/home/duan/study/src/default.yaml");
    let (light_list, light_hash, cfg) = read_config(&f);
    println!("{:?}{:?}{:?}", light_list, light_hash, cfg);
    // 2. 1s循环一次，并发送红绿灯信息到管控中心；红绿灯变化时，发布信息到zenoh中，存储红绿灯信息
    // light_loop(&light_hash, cfg).await;
    // HASHMAP.get(&0).unwrap();

    // let mut map = LIGHTHASH.lock().unwrap();
    // LIGHTHASH = light_hash;
    // let mut map = HASHMAP.lock().unwrap();
    // let mut lgt_hash = LIGHTHASH.lock().unwrap();
    // for (name, light) in light_hash.into_iter() {
    //     LIGHTHASH.insert(name, light);
    // }
    // LIGHTHASH.insert(String::from(""), Light{id: String::from("sd"), name: String::from(""), color: LightColor::RED});
    let mut task: task::JoinHandle<()> = tokio::spawn(light_loop(light_hash.clone(), cfg));
    
    // 启动服务，监听管控中心发送的转换红绿灯信息
    tide::log::start();
    let mut app = tide::new();
    
    // 疑问：
    // 如何在app.at("/rule_change")的调用353行的task（调用目的：修改变量light_hash和cfg,重新开始红绿灯倒计时），以及使用非全局变量


    // 红绿灯管控中心发送红绿灯变动规则
    app.at("/rule_change").get(|mut req: Request<()>|async {
        let rule: RuleMessage = req.body_json().await?;
        println!("Message: {}", rule.name);

        // light_loop(&light_hash, &cfg);
        // println!("{:?}", th);
        // task.abort();  // 不可以这样调用
        tokio::spawn(light_loop(light_hash.clone(), cfg));

        // 根据POST中的请求，修改主灯的配置
        // println!("{:?}", &ss);
        let ret_message = Response {status: 1, message: String::from("")};
        Ok(Body::from_json(&ret_message)?)
    });

    app.listen("127.0.0.1:8080").await.unwrap();

}

