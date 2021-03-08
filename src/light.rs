use serde::{Deserialize, Serialize};
extern crate lazy_static;
use lazy_static::lazy_static;
use std::sync::{Mutex};
use std::collections::HashMap;
use tokio::time::Instant;
use std::convert::TryInto;
use zenoh::*;
use std::time::Duration;

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum LightColor {
    UNKNOWN = 0,
    RED = 1,
    GREEN = 2,
    YELLOW = 3,
}


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Light {
    id: String,
    color: u64,
    remain: i64
}

#[derive(Debug, Clone)]
pub struct LightDuration {
    pub green: i64,
    pub red: i64,
    pub yellow: i64,
    pub unknown: i64
}

#[derive(Debug, Clone)]
pub struct LightStatus {
    pub color: LightColor,
    pub counter: i64,
}


// 灯状态的实现
impl LightStatus {
    // 转灯，每个tick（1秒）调用一次，如果倒计时结束就转灯，并返回true；否则返回false
    pub fn tick(&mut self, light_duration: &LightDuration) -> bool {
        // println!("{:?}",self.counter);
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
    pub static ref LIGHTSTATUS:Mutex<HashMap<String, LightStatus>> = {
        let lgt_status = HashMap::new();
        Mutex::new(lgt_status)
    };

    // 公用的灯循环的时间配置
    pub static ref LIGHTDURATION:Mutex<LightDuration> = {
        let lgt_drtion = LightDuration{
            green: 0,
            red: 0,
            yellow: 0,
            unknown: 0
        };
        Mutex::new(lgt_drtion)
    };

    pub static ref LIGHTGROUP: Mutex<HashMap<String, Vec<String>>> = {
        let map = HashMap::new();
        Mutex::new(map)
    };
}

// 根据灯色获取时长
pub fn get_duration(color: &LightColor) -> i64{
    {
        let lcfg = LIGHTDURATION.lock().unwrap();
        match color {
            &LightColor::RED => lcfg.red,
            &LightColor::GREEN => lcfg.green,
            &LightColor::YELLOW => lcfg.yellow,
            &LightColor::UNKNOWN => lcfg.unknown,
        }
    }
    
}

// 获取相反的灯色
pub fn inverse_color(color: &LightColor, counter: i64) -> LightColor {
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
pub fn init_light_duration(init_color: i32, counter: i64) {
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
pub fn init_lgt_status(lgt_id: &str, init_color: LightColor, remain: i64){
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
pub async fn light_loop(road_id: String, zenoh_url: String) {
    let config = Properties::default();
    let zenoh = Zenoh::new(config.into()).await.unwrap();

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
        workspace.put(&light_path.clone().try_into().unwrap(), zenoh::Value::Json(value_new)).await.unwrap();

        // 发送给CV红绿灯数据
        send(road_id.clone(), zenoh_url.clone(), light_vec).await;

        tokio::time::sleep_until(now.checked_add(Duration::from_secs(1)).unwrap()).await;
    }
}

// 1s发送一次红绿灯结果
async fn send(road_id:String, zenoh_url: String, lgt_info_vec:Vec<Light>) {
    let url = format!("{}{}", zenoh_url, road_id);
    reqwest::Client::new()
    .put(&url)
    .json(&serde_json::json!(lgt_info_vec))
    .send()
    .await.unwrap();
    
}