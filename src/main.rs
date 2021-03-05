use std::fs;
use std::time::Duration;
use std::collections::HashMap;
use futures::prelude::*;
// use futures::select;
// use async_std::task;
use yaml_rust::{YamlLoader};
use serde::{Deserialize, Serialize};
use tide::{Body, Request};
use zenoh::*;
use tokio;
use tokio::time::Instant;
use std::convert::TryInto;
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
    color: u64,
    remain: i64
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
        if self.counter == 0 {
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
    println!("{:?}", lcfg);
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


// 循环灯状态
async fn light_loop(road_id: String, zenoh_url: String) {
    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();
    let light_path = format!("/light/detail/{}", road_id);
    
    //每秒tick
    loop {
        let now = Instant::now();
        let mut light_vec: Vec<Light> = vec![];
        let mut value_new = String::from("{");

        {
            let mut lgt_status_hash = LIGHTSTATUS.lock().unwrap();
            let mut light_group = LIGHTGROUP.lock().unwrap();
            let lgt_duration = LIGHTDURATION.lock().unwrap();

            for (group_name, lgt_id_vec) in light_group.iter_mut() {
                value_new = format!(r#"{}"{}":["#, value_new, group_name);
                // 1. 取出group中的值，为每个灯的剩余时间减一
                // 获取灯的状态
                let lgt_status = lgt_status_hash.get_mut(group_name).unwrap();
                lgt_status.tick(&lgt_duration);
                let color = lgt_status.color;
                let remain = lgt_status.counter;

                // 循环ID，存入每一个红绿灯信息
                for lgt_id in lgt_id_vec {
                    value_new += &String::from("{");
                    let id = lgt_id.clone();
                    value_new = format!(r#"{}"id":"{}","color":{:?},"remain":{:?}"#, value_new, id, color as u64, remain);
                    value_new += &String::from("},");
                    light_vec.push(Light{id: id, color: color as u64, remain: remain});
                }
                let value_len = value_new.len()-1;
                value_new.remove(value_len);
                value_new += &String::from("],");
            }
        }

        let value_len = value_new.len()-1;
        value_new.remove(value_len);
        value_new += &String::from("}");
        // println!("Put Data ('{}': '{}')...\n", light_path, value_new);
        workspace.put(&light_path.clone().try_into().unwrap(), zenoh::Value::Json(value_new)).await.unwrap();

        // 发送给CV红绿灯数据
        send(road_id.clone(), zenoh_url.clone(), light_vec).await;

        tokio::time::sleep_until(now.checked_add(Duration::from_secs(1)).unwrap()).await;
        
    }
}


fn read_config(file_name: &str) -> (String, String) {
    let config_str = fs::read_to_string(file_name).unwrap();
    let config_docs = YamlLoader::load_from_str(config_str.as_str()).unwrap();
    let config = &config_docs[0];
    let light_group_cfg = &config["light_id_group"];
    let road_id =  String::from(config["road_id"].as_str().unwrap());
    let zenoh_url =  String::from(config["server_zenoh_url"].as_str().unwrap());

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
            }
            light_group.insert(group_name.clone(), g_id_list);

            // 初始化LIGHTSTATUS
            if group_name == group_master {
                lgt_status_group_hash.insert(group_name, LightStatus{color: default_color, counter: init_duration});
            } else {
                let in_color = inverse_color(&default_color, init_duration);
                let in_duration = get_duration(&in_color);
                lgt_status_group_hash.insert(group_name, LightStatus{color: in_color, counter: in_duration});
            }
        }

    }
    
    (road_id, zenoh_url)
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
        println!("Message: light_id: {}, color: {}, remain: {}", rule.light_id, rule.color, rule.remain);
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
    app.listen("0.0.0.0:8080").await?;
    Ok(())
}

// 1s发送一次红绿灯结果
async fn send(road_id:String, zenoh_url: String, lgt_info_vec:Vec<Light>) {
    let url = format!("{}{}", zenoh_url, road_id);
    let echo_json = reqwest::Client::new()
    .put(&url)
    .json(&serde_json::json!(lgt_info_vec))
    .send()
    .await.unwrap();
    
    println!("{:#?}", echo_json);
    // Object(
    //     {
    //         "body": String(
    //             "https://docs.rs/reqwest"
    //         ),
    //         "id": Number(
    //             101
    //         ),
    //         "title": String(
    //             "Reqwest.rs"
    //         ),
    //         "userId": Number(
    //             1
    //         )
    //     }
    // )
    // Ok(())

}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let f = String::from("/home/duan/study/src/default.yaml");
    let (road_id, zenoh_url) = read_config(&f);
    
    tokio::spawn(serve_http());
    // tokio::spawn(sub());

    light_loop(road_id, zenoh_url).await;
    
    Ok(())
}
