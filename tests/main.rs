use snowflake::Snowflake;

#[tokio::test]
pub async fn main(){
    pretty_env_logger::init();
    let snowflake: Snowflake = Snowflake::new().await;
    log::debug!("{}", snowflake);
    log::debug!("{}", snowflake.to_string().len());
    loop {

    }
}