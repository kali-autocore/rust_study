use tokio;
mod config;
use config::read_config;
mod light;
mod http_server;

/// 1. 读配置文件
/// 2. 启动修改红绿灯运行规则服务
/// 3. 启动红绿灯运行
#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let f = String::from("/home/duan/study/config.yaml");
    let (road_id, zenoh_url) = read_config(&f);
    
    tokio::spawn(http_server::serve_http());

    light::light_loop(road_id, zenoh_url).await;
    
    Ok(())
}