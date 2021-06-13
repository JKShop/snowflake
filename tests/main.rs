use snowflake::Snowflake;

#[tokio::test]
pub async fn main(){
    pretty_env_logger::init();
    let snowflake: Snowflake = Snowflake::new().await;

    loop {

    }
}