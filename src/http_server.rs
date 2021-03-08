use tide::{Request, Response};
use serde::{Deserialize, Serialize};
use crate::light;
use light::{LightColor};

#[derive(Deserialize, Serialize, Debug)]
struct RuleMessage {
    light_id: String,
    color: i32,
    remain: i64,
}

#[derive(Deserialize, Serialize)]
struct ResponseData {
    status: i32,
    message: String,
}

//http服务，处理修改配置的请求
pub async fn serve_http() -> tide::Result<()> {
    tide::log::start();
    let mut app = tide::new();

    app.at("/").get(|_| async { Ok("OK") });
    
    
    // 红绿灯规则调整
    app.at("/rule_change").post(|mut req: Request<()>| async move {
        let rule: RuleMessage = req.body_form().await?;
        println!("rule cheange, message: light_id: {}, color: {}, remain: {}", rule.light_id, rule.color, rule.remain);
        let remain = rule.remain;
        let color = rule.color;
        let lgt_id = rule.light_id;
        // 1 红 2 绿 3 黄 0 灭灯
        let init_color = match color {
            1 => LightColor::RED,
            2 => LightColor::GREEN,
            3 => LightColor::YELLOW,
            0 => LightColor::UNKNOWN,
            _ => LightColor::UNKNOWN,
        };
        // 重新初始化
        light::init_light_duration(color, remain);
        light::init_lgt_status(&lgt_id, init_color, remain);

        // 返回一个没用的response
        let body_data =  ResponseData {status: 1, message: String::from("")};
        let response = Response::builder(200)
         .body(serde_json::json!(&body_data))
         .header("Content-Type", "application/json")
         .header("Access-Control-Allow-Origin", "*")
         .build();
        Ok(response)
    });

    println!("start server");
    app.listen("0.0.0.0:8080").await?;
    Ok(())
}

