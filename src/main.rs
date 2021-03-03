use std::thread;
use std::fs;
use std::rc::Rc;
use std::time::Duration;
use std::collections::HashMap;
use futures::prelude::*;
// use async_std::task;
use yaml_rust::{YamlLoader};
use serde::{Deserialize, Serialize};
// use tide::prelude::*; // Pulls in the json! macro.
use tide::{Body, Request};
use zenoh::*;
use tokio;
use tokio::task;
use tokio::time::Instant;
use std::convert::TryInto;
// use lazy_static;
// #[macro_use]
extern crate lazy_static;
use lazy_static::lazy_static;
use std::sync::{Mutex};

//灯的颜色枚举
#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
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
    light_id: String,
    color: i32,
    remain: i64,
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
                })(),
                LightColor::UNKNOWN => ()
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
        let lgt_status = HashMap::new();
        Mutex::new(lgt_status)
    };

    // 公用的灯循环的时间配置
    static ref LIGHTDURATION:Mutex<LightDuration> = {
        let lgt_drtion = LightDuration{
            green: 0,
            red: 0,
            yellow: 0,
            unknown: 0
        };
        Mutex::new(lgt_drtion)
    };

    static ref LIGHTGROUP: Mutex<HashMap<String, Vec<String>>> = {
        let map = HashMap::new();
        Mutex::new(map)
    };
}


// 根据灯色获取时长
fn get_duration(color: &LightColor) -> i64{
    println!("31");
    {
        let lcfg = LIGHTDURATION.lock().unwrap();
        println!("32");
        match color {
            &LightColor::RED => lcfg.red,
            &LightColor::GREEN => lcfg.green,
            &LightColor::YELLOW => lcfg.yellow,
            &LightColor::UNKNOWN => lcfg.unknown,
            _ => lcfg.unknown,
        }
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
fn init_light_duration(init_color: i32, counter: i64) {
    let color = init_color.clone();
    let mut lcfg = LIGHTDURATION.lock().unwrap();
    // 1 红 2 绿 3 黄 0 灭灯
    match color {
        2 => lcfg.green = counter,
        1 => lcfg.red = counter,
        3 => lcfg.yellow = counter,
        0 => lcfg.unknown = counter,
        _ => ()
    };
}

/// 根据配置计算所有灯的初始状态status
///1. 根据lgt_id找到灯所属于的组
///2. 更改改组的灯色
/// 
fn init_lgt_status(lgt_id: &str, init_color: LightColor, remain: i64){
    {
        let mut lgt_status = LIGHTSTATUS.lock().unwrap();
        let mut light_group = LIGHTGROUP.lock().unwrap();
        for (group_name, light_id_list) in light_group.iter_mut() {
            for light_id in light_id_list {
                if lgt_id == light_id {
                    // lgt_status.get(&group_name);
                    let mut r_lgt_status = lgt_status.get_mut(&group_name[..]).unwrap();
                    r_lgt_status.color = init_color;
                    r_lgt_status.counter = remain;
                    break;
                }
            }
        }
    }
}


async fn light_loop(road_id: String) {
    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();
    let light_path = format!("/light/detail/{}", road_id);

    //每秒tick
    loop {
        let now = Instant::now();
        {
            let mut lgt_status_list = LIGHTSTATUS.lock().unwrap();
            let lgt_duration = LIGHTDURATION.lock().unwrap();
            let light_group = LIGHTGROUP.lock().unwrap();

            for (group_name, lgt_status) in lgt_status_list.iter_mut() {
                if lgt_status.tick(&lgt_duration) {
                    // 根据group_name,获取Light
                    let light_list: &Vec<String> = light_group.get(group_name).unwrap();
                    
                    // 更新Zenoh中的存储
                    // 1. 取出原有数据
                    let mut lgt_now = workspace.get(&light_path.clone().try_into().unwrap()).await.unwrap();
                    while let Some(data) = lgt_now.next().await {
                        println!(
                            "  {} : {:?} (encoding: {} , timestamp: {})",
                            data.path,
                            data.value,
                            data.value.encoding_descr(),
                            data.timestamp
                        )
                    }

                    // 2. 更新存储的灯的详细信息
                    // path: /light/detail/{road_id}
                    // value: {"id_1": 1, "id_2": 1, "id_3": 2, "id_4": 2,}  light_id: color
                    let mut value = String::from("{");
                    for light_id in light_list {
                        value = format!("{}\"{}\":\"{:?}\",", value, light_id, lgt_status.color);
                    }
                    value += &String::from("}");
                    println!("Put Data ('{}': '{}')...\n", light_path, value);
                    workspace.put(&light_path.clone().try_into().unwrap(), Value::Json(value)).await.unwrap();
                }
            }
        }
        
        // 2. 更新存储的灯的剩余时间
        // path: /road_id/left
        // value: [{"light_id": "12", "color": 1, "remain": 5}]

        tokio::time::sleep_until(now.checked_add(Duration::from_secs(1)).unwrap()).await;
        
    }
    // zenoh.close().await.unwrap();
}


fn read_config(file_name: &str) -> String {
    let config_str = fs::read_to_string(file_name).unwrap();
    let config_docs = YamlLoader::load_from_str(config_str.as_str()).unwrap();
    let config = &config_docs[0];
    let light_group_cfg = &config["light_id_group"];
    let road_id =  String::from(config["road_id"].as_str().unwrap());
    println!("1");
    // 读取灯的变化时间
    let mut light_duration = LIGHTDURATION.lock().unwrap();
    light_duration.green = config["duration"]["green"].as_i64().unwrap();
    light_duration.red = config["duration"]["red"].as_i64().unwrap();
    light_duration.yellow = config["duration"]["yellow"].as_i64().unwrap();
    light_duration.unknown = config["duration"]["unknown"].as_i64().unwrap();
    println!("2");
    // 读取配置中的红绿灯颜色
    let default_color:LightColor;
    match config["color"].as_i64().unwrap() {
        1 => default_color = LightColor::RED,
        2 => default_color = LightColor::GREEN,
        3 => default_color = LightColor::YELLOW,
        0 => default_color = LightColor::UNKNOWN,
        _ => default_color = LightColor::UNKNOWN,
    }
    println!("3");
    let init_duration = get_duration(&default_color);

    // 红绿灯组
    let group_master = config["master"].as_str().unwrap();
    let mut lgt_status_group_hash = LIGHTSTATUS.lock().unwrap();
    let mut light_group = LIGHTGROUP.lock().unwrap();
    println!("4");
    // 读取配置中的红绿灯组
    for (group_name, lgt_id_list) in light_group_cfg.as_hash().unwrap().into_iter() {
        let group_name = String::from(group_name.as_str().unwrap());
        let mut g_id_list = vec![];
        for lgt_id in lgt_id_list.as_vec().unwrap() {
            g_id_list.push(String::from(lgt_id.as_str().unwrap()));
        }
        light_group.insert(group_name.clone(), g_id_list);

        // 初始化LIGHTSTATUS
        if group_name == group_master {
            lgt_status_group_hash.insert(group_name, LightStatus{color: default_color, counter: init_duration});
        } else {
            let in_color = inverse_color(&default_color, init_duration);
            let in_duration = get_duration(&default_color);
            lgt_status_group_hash.insert(group_name, LightStatus{color: in_color, counter: in_duration});
        }
    }
    println!("5");
    road_id
}

//http服务，处理修改配置的请求
async fn serve_http() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    app.at("/").get(|_| async { Ok("Root") });
    
    // 红绿灯规则调整
    app.at("/rule_change").post(|mut req: Request<()>| async move {
        let rule: RuleMessage = req.body_json().await?;
        // light_id: String,
        // color: i32,
        // remain: i64,
        println!("Message: {}", rule.light_id);
        let remain = rule.remain;
        let color = rule.color;
        let lgt_id = rule.light_id;
        // 1 红 2 绿 3 黄 0 灭灯
        let init_color =match color {
            1 => LightColor::RED,
            2 => LightColor::GREEN,
            3 => LightColor::YELLOW,
            0 => LightColor::UNKNOWN,
            _ => LightColor::UNKNOWN,
        };
        // 重新初始化
        init_light_duration(color, remain);
        // 重新初始化灯的状态
        init_lgt_status(&lgt_id, init_color, remain);

        // 返回一个没用的response
        let response =  Response {status: 1, message: String::from("")};

        Ok(Body::from_json(&response)?)
    });

    //http server启动
    println!("start server");
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}


// #[tokio::main]
// async fn main() {
//     // 1. 读取配置文件
//     let f = String::from("/home/duan/study/src/default.yaml");
//     let road_id = read_config(&f);
//     let light_group = LIGHTGROUP.lock().unwrap();
//     println!("{:?}{:?}", road_id, light_group);
//     serve_http();
//     // tokio::spawn(light_loop(road_id));
    
//     // 启动服务主线程自己阻塞在serve_http循环上
//     // use futures::executor::block_on;
//     // block_on(serve_http());

//     // 循环红绿灯
//     // tokio::spawn(async {
//     //     // Force the `Rc` to stay in a scope with no `.await`
//     //     {
//     //         light_loop(road_id).await;
//     //         use futures::executor::block_on;
//     //         block_on(serve_http());
//     //     }

//     //     task::yield_now().await;
//     // });
//     // tokio::spawn(
//     //     async move {
//     //         // Process each socket concurrently.
//     //         light_loop(road_id).await
//     //     }
//     // );
//     // let unsend_data = Rc::new("my unsend data...");
//     // let local = task::LocalSet::new();
//     // local.run_until(async move {
//     //     // let unsend_data = unsend_data.clone();
//     //     // `spawn_local` ensures that the future is spawned on the local
//     //     // task set.
//     //     task::spawn_local(async move {
//     //         // println!("{}", unsend_data);
//     //         light_loop(road_id)
//     //         // ...
//     //     }).await.unwrap();
//     // }).await;

    
//     // let unsend_data = Rc::new("my unsend data...");

//     // // Construct a local task set that can run `!Send` futures.
//     // let local = task::LocalSet::new();

//     // // Run the local task set.
//     // local.run_until(async move {
//     //     let unsend_data = unsend_data.clone();
//     //     // `spawn_local` ensures that the future is spawned on the local
//     //     // task set.
//     //     task::spawn_local(async move {
//     //         println!("{}", unsend_data);
//     //         // ...
//     //     }).await.unwrap();
//     // }).await;
    


// }

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    let f = String::from("/home/duan/study/src/default.yaml");
    let road_id = read_config(&f);
    // {
    //     let light_group = LIGHTGROUP.lock().unwrap();
    //     println!("{:?}", light_group);
    // }
    
    // tide::log::start();
    // let mut app = tide::new();

    // app.at("/").get(|_| async { Ok("Root") });
    
    // // 红绿灯规则调整
    // app.at("/rule_change").post(|mut req: Request<()>| async move {
    //     let rule: RuleMessage = req.body_json().await?;
    //     // light_id: String,
    //     // color: i32,
    //     // remain: i64,
    //     println!("Message: {}", rule.light_id);
    //     let remain = rule.remain;
    //     let color = rule.color;
    //     let lgt_id = rule.light_id;
    //     // 1 红 2 绿 3 黄 0 灭灯
    //     let init_color =match color {
    //         1 => LightColor::RED,
    //         2 => LightColor::GREEN,
    //         3 => LightColor::YELLOW,
    //         0 => LightColor::UNKNOWN,
    //         _ => LightColor::UNKNOWN,
    //     };
    //     // 重新初始化
    //     init_light_duration(color, remain);
    //     // 重新初始化灯的状态
    //     init_lgt_status(&lgt_id, init_color, remain);

    //     // 返回一个没用的response
    //     let response =  Response {status: 1, message: String::from("")};

    //     Ok(Body::from_json(&response)?)
    // });

    // //http server启动
    // println!("start server");
    // app.listen("127.0.0.1:8080").await?;
    Ok(())
}
