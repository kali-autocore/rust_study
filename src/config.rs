///
/// reader cfg
/// 
use std::fs;
use yaml_rust::{YamlLoader};
use crate::light;
use light::{LightColor, LightStatus, LIGHTDURATION, LIGHTGROUP, LIGHTSTATUS};

pub fn read_config(file_name: &str) -> (String, String) {
    println!("begin to read_config");
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
    let init_duration = light::get_duration(&default_color);

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
                let in_color = light::inverse_color(&default_color, init_duration);
                let in_duration = light::get_duration(&in_color);
                lgt_status_group_hash.insert(group_name, LightStatus{color: in_color, counter: in_duration});
            }
        }

    }
    
    println!("read config ok");
    (road_id, zenoh_url)
}