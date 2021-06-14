# Snowflake
- Generates snowflake id's
## Usage

Set env ```SNOWFLAKE.COORDINATOR``` to point to your coordinator server !
 
### Cargo.toml
```toml
[dependencies]
snowflake="1.0.0"
```

### main.rs
```rust
use snowflake::Snowflake;

fn example(){
    /*Implements Debug and Display*/
    let snowflake = Snowflake = Snowflake::new();
    println!("{}", snowflake);
}
```