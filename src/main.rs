use std::fs;
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
use tokio::time::Instant;
use std::convert::TryInto;
// use lazy_static;
// #[macro_use]
extern crate lazy_static;
use lazy_static::lazy_static;
use std::sync::{Mutex};
use serde_json::{Value};

//灯的颜色枚举
#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
enum LightColor {
    UNKNOWN = 0,
    RED = 1,
    GREEN = 2,
    YELLOW = 3,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
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

fn get_color(color: &u64) -> LightColor {
    match color {
        1 => LightColor::RED,
        2 => LightColor::GREEN,
        3 => LightColor::YELLOW,
        0 => LightColor::UNKNOWN,
        _ => LightColor::UNKNOWN
    }
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
    {
        let lcfg = LIGHTDURATION.lock().unwrap();
        match color {
            &LightColor::RED => lcfg.red,
            &LightColor::GREEN => lcfg.green,
            &LightColor::YELLOW => lcfg.yellow,
            &LightColor::UNKNOWN => lcfg.unknown,
            // _ => lcfg.unknown,
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


async fn light_loop(road_id: String, lgt_id_list: &Vec<String>) {
    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();
    let light_path = format!("/light/detail/{}", road_id);

    let mut light_list_now: Vec<Light> = Vec::new();

    // // 初始化zenoh中灯的状态
    {
        let lgt_status_list = LIGHTSTATUS.lock().unwrap();
        let lgt_duration = LIGHTDURATION.lock().unwrap();
        let mut light_group = LIGHTGROUP.lock().unwrap();

        for (group_name, light_id_list) in light_group.iter_mut() {
            let lgt_color = lgt_status_list.get(group_name).unwrap();
            for lgt_id in light_id_list {
                let id = lgt_id.clone();
                light_list_now.push(Light{id: id, color:lgt_color.color.clone()});
            }
        }
    }

    // 初始化zenoh
    if !light_list_now.is_empty() {
        let mut value = String::from("{");
        for light in light_list_now {
            value = format!("{}\"{}\":{:?},", value, light.id, light.color as u64);
        }
        let value_len = value.len()-1;
        value.remove(value_len);
        value += &String::from("}");
        println!("Put Data ('{}': '{}')...\n", light_path, value);
        workspace.put(&light_path.clone().try_into().unwrap(), zenoh::Value::Json(value)).await.unwrap();
    }

    //每秒tick
    loop {
        let now = Instant::now();
        {
            let mut lgt_status_list = LIGHTSTATUS.lock().unwrap();
            let lgt_duration = LIGHTDURATION.lock().unwrap();
            let mut light_group = LIGHTGROUP.lock().unwrap();
            

            let mut light_list: Vec<Light> = Vec::new();

            for (group_name, lgt_status) in lgt_status_list.iter_mut() {
                if lgt_status.tick(&lgt_duration) {

                    // 根据group_name,获取Light
                    let light_id_list_now = light_group.get(group_name).unwrap();
                    println!("{:?}", light_id_list_now);
                    for (g, l) in light_group.iter_mut() {
                        println!("{:?}", l);
                    }

                    // // 更新Zenoh中的存储
                    // 1. 取出原有数据
                    let mut lgt_now = workspace.get(&light_path.clone().try_into().unwrap()).await.unwrap();
                    while let Some(data) = lgt_now.next().await {
                        // Data { path: Path { p: "/light/detail/1111111" }, value: Json("{\"3\":\"RED\",\"4\":\"RED\",}"), timestamp: 603f27c1d4362790/9679372789864994B044C9B9FE82FB4B } 
                        let data: &str = &data.value.clone().encode_to_string().2.to_owned();
                        let lgt_value: Value = serde_json::from_str(data).unwrap();
                        println!("{:?}", lgt_value);
                        println!("{:?}", lgt_value["light_1"]);
                        // {
                        //     let mut light_group = LIGHTGROUP.lock().unwrap();
                        //     for (g_name, lgt_id_vec) in light_group.iter_mut(){
                        //         if lgt_id_vec == light_id_list_now {
                        //             for lgt_id in lgt_id_vec {
                        //                 light_list.push(Light{id: lgt_id.clone(), color: lgt_status.color});
                        //             }
                        //         } else {
                        //             println!("{:?}", lgt_id_vec);
                        //             for lgt_id in lgt_id_vec {
                        //             //     let lgt_color = &lgt_value[&lgt_id];
                        //             //     let color: LightColor = get_color(&lgt_color.as_u64().unwrap());
                        //             //     light_list.push(Light{id: lgt_id.clone(), color: color});
                        //             }
                        //         }
                                    
                        //     }
                        // }
                        
                    

                    // for lgt_id in lgt_id_list {  // 循环所有的light id
                    //     let lgt_color = &lgt_value[lgt_id];
                    //     // println!("{:?}", lgt_color);
                    //     // println!("{:?}", lgt_color.is_u64());
                    //     for lgt_id_now in light_id_list_now {  // 循环当前变化的灯的id
                    //         if lgt_id == lgt_id_now {
                    //             // 更换该灯的颜色
                    //             light_list.push(Light{id: lgt_id.clone(), color: lgt_status.color});
                    //             continue;
                    //         }
                    //     }
                    //     // 存储剩下的灯颜色
                    //     let color: LightColor = get_color(&lgt_color.as_u64().unwrap());
                    //     light_list.push(Light{id: lgt_id.clone(), color: color});
                    // }
                    
                    }

                    // 2. 更新存储的灯的详细信息
                    // path: /light/detail/{road_id}
                    // value: {"id_1": 1, "id_2": 1, "id_3": 2, "id_4": 2,}  light_id: color
                    let lgt_list_temp = light_list.clone();
                    if !lgt_list_temp.is_empty() {
                        let mut value = String::from("{");
                        for light in lgt_list_temp {
                            value = format!("{}\"{}\":{:?},", value, light.id, light.color as u64);
                        }
                        let value_len = value.len()-1;
                        value.remove(value_len);
                        value += &String::from("}");
                        println!("Put Data ('{}': '{}')...\n", light_path, value);
                        workspace.put(&light_path.clone().try_into().unwrap(), zenoh::Value::Json(value)).await.unwrap();
                    }
                    
                }
            }
        }
        
        // 2. 更新存储的灯的剩余时间
        // path: /road_id/left
        // value: [{"light_id": "12", "color": 1, "remain": 5}]

        tokio::time::sleep_until(now.checked_add(Duration::from_secs(1)).unwrap()).await;
        
    }
}


fn read_config(file_name: &str) -> (String, Vec<String>) {
    let config_str = fs::read_to_string(file_name).unwrap();
    let config_docs = YamlLoader::load_from_str(config_str.as_str()).unwrap();
    let config = &config_docs[0];
    let light_group_cfg = &config["light_id_group"];
    let road_id =  String::from(config["road_id"].as_str().unwrap());
    let mut light_id_list_ret = Vec::new();

    // 读取灯的变化时间
    {
        let mut light_duration = LIGHTDURATION.lock().unwrap();
        light_duration.green = config["duration"]["green"].as_i64().unwrap();
        light_duration.red = config["duration"]["red"].as_i64().unwrap();
        light_duration.yellow = config["duration"]["yellow"].as_i64().unwrap();
        light_duration.unknown = config["duration"]["unknown"].as_i64().unwrap();
    }
    
    // 读取配置中的红绿灯颜色
    let default_color:LightColor;
    match config["color"].as_i64().unwrap() {
        1 => default_color = LightColor::RED,
        2 => default_color = LightColor::GREEN,
        3 => default_color = LightColor::YELLOW,
        0 => default_color = LightColor::UNKNOWN,
        _ => default_color = LightColor::UNKNOWN,
    }
    let init_duration = get_duration(&default_color);

    // 红绿灯组
    let group_master = config["master"].as_str().unwrap();
    {
        let mut light_group = LIGHTGROUP.lock().unwrap();
        let mut lgt_status_group_hash = LIGHTSTATUS.lock().unwrap();

        // 读取配置中的红绿灯组
        for (group_name, lgt_id_list) in light_group_cfg.as_hash().unwrap().into_iter() {
            let group_name = String::from(group_name.as_str().unwrap());
            let mut g_id_list = vec![];
            for lgt_id in lgt_id_list.as_vec().unwrap() {
                g_id_list.push(String::from(lgt_id.as_str().unwrap()));
                // 获取所有的灯ID
                light_id_list_ret.push(String::from(lgt_id.as_str().unwrap()));
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

    }
    
    (road_id, light_id_list_ret)
}

//http服务，处理修改配置的请求
async fn serve_http() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    app.at("/").get(|_| async { Ok("OK") });
    
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

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let f = String::from("/home/duan/study/src/default.yaml");
    let (road_id, lgt_id_list) = read_config(&f);
    {
        let light_group = LIGHTGROUP.lock().unwrap();
        println!("{:?}", light_group);
    }
    tokio::spawn(serve_http());
    light_loop(road_id, &lgt_id_list).await;
    Ok(())
}
